//! Something outside the app asking the user for a yes: the SSH agent before a
//! signature, the CLI before it hands a command its secrets.
//!
//! The asking side opens a question, shows a card (an event), and waits — on a
//! plain thread, never an async one — for the answer or the timeout, and a
//! timeout is a no. The card answers by id, once.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

static WAITING: Mutex<Option<HashMap<String, Sender<bool>>>> = Mutex::new(None);

/// How long a question waits before it is a no.
pub const WAIT: Duration = Duration::from_secs(60);

pub struct Question {
    pub id: String,
    answer: Receiver<bool>,
}

/// Open a question. Show its card, then [`Question::wait`].
pub fn ask() -> Question {
    let id = hex::encode(super::crypto::random_bytes::<8>().unwrap_or_default());
    let (tx, rx) = channel();
    WAITING.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(id.clone(), tx);
    Question { id, answer: rx }
}

impl Question {
    /// The user's answer, or no after `timeout`. Blocks the calling thread.
    pub fn wait(self, timeout: Duration) -> bool {
        let yes = self.answer.recv_timeout(timeout).unwrap_or(false);
        WAITING.lock().unwrap_or_else(|p| p.into_inner()).as_mut().map(|m| m.remove(&self.id));
        yes
    }

    /// The card could not be shown: no one can answer, so it is a no.
    pub fn abandon(self) -> bool {
        WAITING.lock().unwrap_or_else(|p| p.into_inner()).as_mut().map(|m| m.remove(&self.id));
        false
    }
}

/// Answer a question by id. False when no question has that id — answered
/// already, timed out, or never asked.
pub fn answer(id: &str, yes: bool) -> bool {
    let mut guard = WAITING.lock().unwrap_or_else(|p| p.into_inner());
    guard.as_mut().and_then(|m| m.remove(id)).is_some_and(|tx| tx.send(yes).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_reaches_its_question_once() {
        let q = ask();
        let id = q.id.clone();
        let waiter = std::thread::spawn(move || q.wait(Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(20));
        assert!(answer(&id, true));
        assert!(waiter.join().unwrap());
        assert!(!answer(&id, true), "answered twice");
    }

    #[test]
    fn silence_is_a_no() {
        let q = ask();
        assert!(!q.wait(Duration::from_millis(30)));
        assert!(!answer("never-asked", true));
    }
}
