//! Calendar time rules.
//!
//! Everything that decides *when* an event happens lives here, and only here.
//! It used to live twice — once in TypeScript so the grid could draw, once in
//! `chat_engine` so the reminder loop could fire — and the two disagreed. A
//! monthly series starting on the 31st was drawn on 28 February and never
//! notified anyone, because one side clamped to the end of a short month and
//! the other compared the strings `"31"` and `"28"`.
//!
//! The front end no longer owns a copy. It asks for a date range and renders
//! what comes back.

pub mod ical;
pub mod ics;
pub mod recurrence;
pub mod reminders;
pub mod scheduler;
pub mod rrule;
pub mod tz;
