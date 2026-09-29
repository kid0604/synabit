//! Where keys live while the Safe is open: a page the operating system is told
//! never to write to swap.
//!
//! # What this is for
//!
//! A key in ordinary memory can be paged out to disk under memory pressure,
//! and a swap file outlives the process — it may be read long after the Safe
//! was locked, from a disk nobody thought held secrets. `mlock` asks the kernel
//! to keep a page in RAM. Every `Key` lives in one such page.
//!
//! It also fixes a subtler leak. A `[u8; 32]` held by value is copied every
//! time the value moves, and only the last copy is wiped on drop. A key that
//! lives at one address in a page it never leaves has one copy to wipe.
//!
//! # What it is not
//!
//! Not protection from a process that can read this one's memory — nothing in
//! user space is. And not a guarantee: `mlock` can be refused (a low
//! `RLIMIT_MEMLOCK`; Android allows 64 KiB, which this one page fits), and on
//! Windows this build does not call `VirtualLock` yet. Either way a key falls
//! back to an ordinary heap allocation that is still wiped on drop, and the
//! first fallback is logged.
//!
//! # Core dumps
//!
//! A crash dump is every page of the process, written to disk. [`harden`]
//! turns them off for this process the first time a Safe is opened, on the
//! platforms where that is one call.

use std::sync::Mutex;

pub const KEY_LEN: usize = 32;
const PAGE: usize = 4096;
const SLOTS: usize = PAGE / KEY_LEN;

struct Arena {
    base: *mut u8,
    used: [bool; SLOTS],
}

// The arena's page is only reached through `Slot`s, each owned by exactly one
// `Key`, and slot bookkeeping is behind the mutex.
unsafe impl Send for Arena {}

static ARENA: Mutex<Option<Arena>> = Mutex::new(None);
static WARNED: std::sync::Once = std::sync::Once::new();

fn warn_once(why: &str) {
    WARNED.call_once(|| log::warn!("[Safe] keys are kept in ordinary memory: {why}"));
}

impl Arena {
    fn new() -> Option<Arena> {
        let layout = std::alloc::Layout::from_size_align(PAGE, PAGE).ok()?;
        // SAFETY: a non-zero size and a valid alignment.
        let base = unsafe { std::alloc::alloc_zeroed(layout) };
        if base.is_null() {
            warn_once("could not allocate a page for them");
            return None;
        }
        if !lock_page(base) {
            // SAFETY: allocated just above with this layout, never shared.
            unsafe { std::alloc::dealloc(base, layout) };
            return None;
        }
        Some(Arena { base, used: [false; SLOTS] })
    }
}

#[cfg(unix)]
fn lock_page(base: *mut u8) -> bool {
    // SAFETY: `base` is a live, page-aligned allocation of `PAGE` bytes.
    let locked = unsafe { libc::mlock(base as *const libc::c_void, PAGE) } == 0;
    if !locked {
        warn_once(&format!("mlock was refused ({})", std::io::Error::last_os_error()));
    }
    locked
}

#[cfg(not(unix))]
fn lock_page(_base: *mut u8) -> bool {
    warn_once("page locking is not implemented on this platform yet");
    false
}

/// Where one key's 32 bytes are.
pub(crate) enum Slot {
    Locked { ptr: *mut u8, index: usize },
    Heap(Box<[u8; KEY_LEN]>),
}

// A slot's bytes belong to exactly one `Key`, which is what owns the `Slot`.
unsafe impl Send for Slot {}
unsafe impl Sync for Slot {}

impl Slot {
    /// A slot holding `bytes`. The caller wipes its own copy.
    pub(crate) fn new(bytes: &[u8; KEY_LEN]) -> Slot {
        let mut guard = ARENA.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            *guard = Arena::new();
        }
        if let Some(arena) = guard.as_mut() {
            if let Some(index) = arena.used.iter().position(|u| !u) {
                arena.used[index] = true;
                // SAFETY: `index < SLOTS`, so the 32 bytes are inside the page.
                let ptr = unsafe { arena.base.add(index * KEY_LEN) };
                // SAFETY: the slot is ours alone and 32 bytes long.
                unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, KEY_LEN) };
                return Slot::Locked { ptr, index };
            }
            warn_once("more keys are open at once than one locked page holds");
        }
        Slot::Heap(Box::new(*bytes))
    }

    pub(crate) fn bytes(&self) -> &[u8; KEY_LEN] {
        match self {
            // SAFETY: the slot stays reserved for as long as `self` lives.
            Slot::Locked { ptr, .. } => unsafe { &*(*ptr as *const [u8; KEY_LEN]) },
            Slot::Heap(b) => b,
        }
    }

    #[cfg(test)]
    pub(crate) fn is_locked(&self) -> bool {
        matches!(self, Slot::Locked { .. })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        match self {
            Slot::Locked { ptr, index } => {
                // `write_volatile` so the wipe of memory about to be reused is
                // not optimised away.
                for i in 0..KEY_LEN {
                    // SAFETY: inside this slot, which is still ours.
                    unsafe { std::ptr::write_volatile(ptr.add(i), 0) };
                }
                std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
                let mut guard = ARENA.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(arena) = guard.as_mut() {
                    arena.used[*index] = false;
                }
            }
            Slot::Heap(b) => zeroize::Zeroize::zeroize(&mut **b),
        }
    }
}

/// Turn off core dumps for this process. Idempotent; best effort.
pub fn harden() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(unix)]
        {
            let none = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            // SAFETY: a valid pointer to a valid struct.
            if unsafe { libc::setrlimit(libc::RLIMIT_CORE, &none) } != 0 {
                log::warn!("[Safe] could not turn off core dumps: {}", std::io::Error::last_os_error());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slots are shared by the whole process: a test that frees one and
    /// looks at it must not race a test that takes it.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        ONE_AT_A_TIME.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn a_slot_holds_its_bytes_and_wipes_them() {
        let _alone = alone();
        // A pattern no other test uses: another test may take the slot the
        // moment it is released, so what is checked is that ours is gone.
        let ours = [0xA5; KEY_LEN];
        let slot = Slot::new(&ours);
        assert_eq!(slot.bytes(), &ours);
        if let Slot::Locked { ptr, .. } = &slot {
            let ptr = *ptr;
            drop(slot);
            // SAFETY: the page lives for the whole process; the slot was
            // released, not freed.
            let after = unsafe { std::slice::from_raw_parts(ptr, KEY_LEN) };
            assert_ne!(after, &ours, "the slot was not wiped");
        }
    }

    /// On the machines this is developed on (macOS, Linux) the page locks.
    #[cfg(unix)]
    #[test]
    fn keys_live_in_locked_memory_on_unix() {
        let _alone = alone();
        let slot = Slot::new(&[1; KEY_LEN]);
        assert!(slot.is_locked(), "mlock was refused on this machine");
    }

    #[test]
    fn more_keys_than_slots_fall_back_rather_than_fail() {
        let _alone = alone();
        let many: Vec<Slot> = (0..SLOTS + 8).map(|i| Slot::new(&[i as u8; KEY_LEN])).collect();
        for (i, slot) in many.iter().enumerate() {
            assert_eq!(slot.bytes(), &[i as u8; KEY_LEN]);
        }
        assert!(many.iter().any(|s| !s.is_locked()));
    }
}
