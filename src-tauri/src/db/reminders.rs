use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use std::collections::HashSet;

/// How long a delivery record is kept.
///
/// Long enough that a machine off for a fortnight does not re-announce
/// everything it missed on the way back, short enough that the table stays
/// small forever rather than growing with the age of the vault.
const KEEP_DAYS: i64 = 30;

impl DbBridge {
    /// The reminders already announced on or after `since` (a unix second).
    ///
    /// This replaced reading and parsing every message file in the vault on
    /// every tick of the reminder loop.
    pub fn delivered_reminders(&self, since: i64) -> AppResult<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT delivery_key FROM reminder_deliveries WHERE delivered_at >= ?1")
            .map_err(|e| AppError::General(format!("DB Query Error (deliveries): {}", e)))?;
        let rows = stmt
            .query_map(params![since], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::General(format!("DB Map Error (deliveries): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// Record that these reminders have been announced.
    ///
    /// Written in one transaction: a crash between two notifications must not
    /// leave one of them able to arrive a second time.
    pub fn record_reminder_deliveries(&mut self, keys: &[String], at: i64) -> AppResult<()> {
        if keys.is_empty() {
            return Ok(());
        }
        let tx = self
            .conn
            .transaction()
            .map_err(|e| AppError::General(format!("DB Tx Error (deliveries): {}", e)))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO reminder_deliveries (delivery_key, delivered_at)
                     VALUES (?1, ?2)
                     ON CONFLICT(delivery_key) DO NOTHING",
                )
                .map_err(|e| AppError::General(format!("DB Prepare Error (deliveries): {}", e)))?;
            for key in keys {
                stmt.execute(params![key, at])
                    .map_err(|e| AppError::General(format!("DB Write Error (deliveries): {}", e)))?;
            }
        }
        tx.commit()
            .map_err(|e| AppError::General(format!("DB Commit Error (deliveries): {}", e)))
    }

    /// Drop records old enough that nothing will ask about them again.
    pub fn prune_reminder_deliveries(&self, now: i64) -> AppResult<usize> {
        let cutoff = now - KEEP_DAYS * 24 * 60 * 60;
        self.conn
            .execute(
                "DELETE FROM reminder_deliveries WHERE delivered_at < ?1",
                params![cutoff],
            )
            .map_err(|e| AppError::General(format!("DB Delete Error (deliveries): {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use crate::db::DbBridge;

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    #[test]
    fn a_reminder_is_remembered_once_it_has_been_announced() {
        let mut db = db();
        assert!(db.delivered_reminders(0).unwrap().is_empty());

        db.record_reminder_deliveries(&["Events/a.md_2026-03-20_15m".to_string()], 1_000)
            .unwrap();

        let seen = db.delivered_reminders(0).unwrap();
        assert!(seen.contains("Events/a.md_2026-03-20_15m"));
        assert!(!seen.contains("Events/a.md_2026-03-20_0m"), "a different offset is a different reminder");
    }

    /// The loop writes what it just announced on every tick, and the same
    /// reminder can be in flight twice if a tick overlaps. Recording it twice
    /// must not be an error.
    #[test]
    fn recording_the_same_reminder_twice_is_not_an_error() {
        let mut db = db();
        let keys = vec!["Events/a.md_2026-03-20_0m".to_string()];
        db.record_reminder_deliveries(&keys, 1_000).unwrap();
        db.record_reminder_deliveries(&keys, 2_000).unwrap();
        assert_eq!(db.delivered_reminders(0).unwrap().len(), 1);
    }

    #[test]
    fn only_records_since_the_cutoff_are_returned() {
        let mut db = db();
        db.record_reminder_deliveries(&["old".to_string()], 1_000).unwrap();
        db.record_reminder_deliveries(&["new".to_string()], 9_000).unwrap();

        let recent = db.delivered_reminders(5_000).unwrap();
        assert!(recent.contains("new"));
        assert!(!recent.contains("old"));
    }

    /// Without pruning this table would grow for as long as the vault exists.
    #[test]
    fn records_older_than_a_month_are_dropped() {
        let mut db = db();
        let now = 100 * 24 * 60 * 60;
        let long_ago = now - 60 * 24 * 60 * 60;
        db.record_reminder_deliveries(&["ancient".to_string()], long_ago).unwrap();
        db.record_reminder_deliveries(&["recent".to_string()], now).unwrap();

        assert_eq!(db.prune_reminder_deliveries(now).unwrap(), 1);
        let left = db.delivered_reminders(0).unwrap();
        assert!(left.contains("recent"));
        assert!(!left.contains("ancient"));
    }

    #[test]
    fn writing_nothing_is_allowed_and_does_nothing() {
        let mut db = db();
        db.record_reminder_deliveries(&[], 1_000).unwrap();
        assert!(db.delivered_reminders(0).unwrap().is_empty());
    }
}
