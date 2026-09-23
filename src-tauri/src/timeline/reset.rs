//! Tearing the timeline down to start it again.
//!
//! # What this is for
//!
//! A timeline read by a poor model is a timeline of poor moments, and there is
//! no patience in going through two hundred of them one at a time to say "no,
//! not that one either". So: put every moment in the trash, forget every
//! reading and every decision, and let a better model read the vault again
//! from nothing.
//!
//! # What it does not touch
//!
//! Notes, people, tasks — everything the moments were read *from* — are the
//! point of the vault and are never touched. Nor is:
//!
//! - **the settings** (`Timeline/extract.json`): the kinds of moment, the
//!   model, the folders. Starting again is not reconfiguring.
//! - **the evidence ledger** (`Timeline/ledger/`): a hash chain about files,
//!   not about moments.
//! - **transcripts and captions** (`surrogates` in the month files): minutes
//!   of a model's time each, and about a recording rather than about a life.
//!   The month files are rewritten without their moments rather than deleted,
//!   so these stay.
//!
//! # Nothing is destroyed
//!
//! Every file goes to the vault's trash, where the Files app can bring it
//! back. What is cleared in `timeline.db` is an index: it is rebuilt from the
//! vault on the next catch-up, which is exactly what makes losing it cheap.

use serde::Serialize;

use super::extract;
use crate::db::{DbBridge, DbState};
use crate::error::{AppError, AppResult};

fn sql(e: rusqlite::Error) -> AppError {
    AppError::General(format!("timeline reset: {e}"))
}

/// What starting again would take away, counted before anything is touched.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Plan {
    /// Moments kept, each a file of its own.
    pub moments: usize,
    /// Proposals waiting, and the ones already decided about.
    pub proposals: usize,
    /// Readings recorded: what was read, so it is not read twice.
    pub readings: usize,
    /// Decisions: kept, declined.
    pub decisions: usize,
    /// Month files that hold them.
    pub month_files: usize,
    /// Transcripts and captions in those files, which stay.
    pub surrogates: usize,
}

pub fn plan(db: &DbBridge, conn: &rusqlite::Connection, vault_path: &str) -> AppResult<Plan> {
    extract::ensure_schema(conn)?;
    let count = |table: &str| -> AppResult<usize> {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .map_err(sql)
    };
    Ok(Plan {
        moments: super::moments::kept(db)?.len(),
        proposals: conn
            .query_row("SELECT COUNT(*) FROM events WHERE source = 'extract'", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .map_err(sql)?,
        readings: count("extract_runs")?,
        decisions: extract::reviewed(vault_path).len(),
        month_files: extract::month_files(vault_path).len(),
        surrogates: count("media_surrogates")?,
    })
}

/// What starting again actually took away.
#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub struct Done {
    pub moments: usize,
    pub month_files: usize,
    pub review_files: usize,
    pub surrogates_kept: usize,
    /// What could not be moved, and why. Nothing else is stopped by it.
    pub failed: Vec<String>,
}

/// Put every moment in the trash, forget every reading, and empty the index.
///
/// `expect_moments` is what the person was shown when they were asked. If the
/// vault has gained or lost a moment since — a sync landing, another window —
/// this refuses rather than doing something other than what they agreed to.
pub fn reset(
    state: &DbState,
    conn: &rusqlite::Connection,
    vault_path: &str,
    expect_moments: Option<usize>,
) -> AppResult<Done> {
    let mut done = Done::default();
    let kept = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        super::moments::kept(&db)?
    };
    if let Some(expected) = expect_moments {
        if expected != kept.len() {
            return Err(AppError::General(format!(
                "The vault holds {} moments now, not the {expected} you were shown. Nothing was touched; look again.",
                kept.len()
            )));
        }
    }

    // 1. The moments themselves, to the trash.
    for moment in &kept {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        match crate::commands::trash::apply_trash(&db, vault_path, &moment.path) {
            Ok(_) => done.moments += 1,
            Err(e) => done.failed.push(format!("{}: {e}", moment.path)),
        }
    }

    // 2. The month files: their moments and readings go, their transcripts stay.
    for (rel, _) in extract::month_files(vault_path) {
        match extract::forget_readings(vault_path, &rel) {
            Ok(surrogates) => {
                done.month_files += 1;
                done.surrogates_kept += surrogates;
            }
            Err(e) => done.failed.push(format!("{rel}: {e}")),
        }
    }

    // 3. The decisions. Kept, declined — all of it was about proposals that no
    //    longer exist, and holding on to them would keep the new reading from
    //    ever offering the same moment again.
    for rel in extract::review_files(vault_path) {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        match crate::commands::trash::apply_trash(&db, vault_path, &rel) {
            Ok(_) => done.review_files += 1,
            Err(e) => done.failed.push(format!("{rel}: {e}")),
        }
    }

    // 4. The index, which is only ever a reading of the two above.
    // The corrections table is made the first time somebody puts a reading
    // right, so it may not be there at all.
    super::reader::corrections(conn)?;
    conn.execute_batch(
        "DELETE FROM event_links;
         DELETE FROM events;
         DELETE FROM extract_runs;
         DELETE FROM extract_drops;
         DELETE FROM month_files;
         DELETE FROM node_sources;
         DELETE FROM reader_corrections;",
    )
    .map_err(|e| AppError::General(format!("timeline reset: {e}")))?;

    // 5. And read the vault again, which now says nothing about moments.
    extract::load(conn, vault_path)?;
    Ok(done)
}

#[cfg(test)]
mod tests;
