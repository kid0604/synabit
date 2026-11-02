//! A room on this machine: real commands, and a kernel that says no.
//!
//! Syn's tools can only ever do what somebody wrote a tool for. A shell can do
//! what a person at the computer can, which is why it is also what an injected
//! instruction wants most. So commands run inside a Seatbelt profile: no
//! network, no file outside the room's own folder, no other process, no system
//! service. What keeps a secret in is the kernel, not a list of commands.
//!
//! Nothing here is reachable from a model yet. This is the part that has to
//! hold before anything else is worth building: [`profile`] writes the rules,
//! [`cell`] runs a command under them, [`watch`] finds what a command left
//! behind, and [`selftest`] attacks the result and refuses to call it safe
//! unless the attacks failed for the right reason. [`export`] is the one way a
//! file leaves, and [`sanitize`] is what a person is shown of a room's words.
//!
//! macOS only, and macOS 14 or later: 13 does not know the `file-lock` rule, so
//! the profile does not compile there and [`selftest`] reports the room broken.
//! The app itself still runs on 13.3.

pub mod cell;
pub mod export;
pub mod profile;
pub mod sanitize;
pub mod selftest;
pub mod watch;
