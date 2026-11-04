//! A room on this machine: real commands, and a kernel that says no.
//!
//! Syn's tools can only ever do what somebody wrote a tool for. A shell can do
//! what a person at the computer can, which is why it is also what an injected
//! instruction wants most. So commands run inside a Seatbelt profile: no
//! network, no file outside the room's own folder, no other process, no system
//! service. What keeps a secret in is the kernel, not a list of commands.
//!
//! [`profile`] writes the rules, [`cell`] runs a command under them, [`watch`]
//! finds what a command left behind, and [`selftest`] attacks the result and
//! refuses to call it safe unless the attacks failed for the right reason.
//! [`export`] is the one way a file leaves, [`inside`] is how the app reads and
//! writes a room's files without following anything the room left, and
//! [`sanitize`] is what a person is shown of a room's words. [`rooms`] is the way
//! in from the rest of the app: it hands out a room only after the attacks have
//! failed on this macOS, remembers which rooms hold somebody else's words, and
//! can stop them all.
//!
//! [`tool`] is the model's way in, from the app only so far: a command, a file
//! in or out, each weighed by the gate like any tool and run by the engine.
//!
//! [`find`] and [`status`] run no command the model wrote, so they need no room.
//! They look at the machine, and that is all they do.
//!
//! macOS only, and macOS 14 or later: 13 does not know the `file-lock` rule, so
//! the profile does not compile there and [`selftest`] reports the room broken.
//! The app itself still runs on 13.3.

pub mod apply;
pub mod cell;
pub mod doors;
pub mod env;
pub mod export;
pub mod fd;
pub mod find;
pub mod inside;
pub mod profile;
pub mod proxy;
pub mod rehearse;
#[cfg(test)]
mod rehearse_tests;
pub mod rooms;
pub mod sanitize;
pub mod selftest;
pub mod status;
pub mod tool;
pub mod watch;
