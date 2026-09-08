//! What the user tells Syn to always do — as a file they can open.
//!
//! # Why this is not a settings field
//!
//! It was one. `SynSettings::custom_system_prompt` is a string inside
//! `Syn/settings.json`, and that left this app with a plain contradiction at
//! its centre: a skill is a Markdown file the user can read, diff and revert;
//! a memory is a Markdown file the user can read, argue with and delete; and
//! the thing that shapes **every answer Syn ever gives** was a field in a JSON
//! blob, reachable only through a textarea in a settings modal.
//!
//! `{vault}/SYN.md` is the same idea as the rest of the vault applied to the
//! one place it had not been. It syncs, it has version history through the CRDT
//! log, it opens in any editor, it is findable by Nexus, and it can be edited
//! on a phone with the app closed.
//!
//! # Why it is read from disk rather than through the index
//!
//! Because somebody has to be able to *just make the file*. Reading it as a
//! node would mean it only counted once the scanner had seen it and only if it
//! carried the right `type:`, and "create SYN.md and type what you want" is the
//! whole affordance.
//!
//! It is still indexed, like any other Markdown in the vault, and that is a
//! feature rather than a leak: it is a document about how the two of them work,
//! and it belongs in search beside everything else.
//!
//! # Why frontmatter is stripped
//!
//! Not for tidiness — for correctness. Sync writes `node_id` into the
//! frontmatter of every Markdown file it touches, so a synced `SYN.md` grows a
//! block the user never typed. Feeding that to the model would put a uuid at
//! the top of its standing instructions.
//!
//! # What this deliberately does not do
//!
//! It does not merge with `custom_system_prompt`. Two sources for one thing is
//! how they drift; the file wins whenever it exists, and the setting is what a
//! vault that predates this still has. `migrate` moves one to the other, once.

use std::path::{Path, PathBuf};

/// Where the file lives.
///
/// The vault root, not `Syn/`: `is_in_unscanned_dir` skips a directory named
/// exactly `Syn`, so a file there would never be indexed — and more to the
/// point, this is a document the user is meant to find without being told where
/// to look.
pub const FILE: &str = "SYN.md";

/// How much of it rides in every prompt.
///
/// Four thousand characters — larger than memory's 3,200, because this is the
/// user speaking directly rather than Syn's guesses about them, and smaller
/// than retrieval's 12,000, because it is paid for on every single turn.
///
/// Past it the text is cut and the prompt says so. A silently truncated
/// instruction is one the user believes is in force and is not.
pub const BUDGET_CHARS: usize = 4_000;

pub fn path(vault_path: &str) -> PathBuf {
    Path::new(vault_path).join(FILE)
}

/// A starting point, offered on an empty file and never written without asking.
///
/// # Why a template rather than a blank page
///
/// The file has existed since the correction door went in, and a blank page
/// with the caption *"tell Syn how to work with you"* is a page nobody fills
/// in. Not because they have nothing to say — because "how should an assistant
/// behave" is too large a question to answer from nothing, and the sentences
/// that actually change an answer are not the ones that come to mind first.
///
/// # Why these four questions
///
/// They are the four that decide behaviour rather than tone, and each of them
/// is something Syn otherwise decides on the user's behalf and never mentions:
///
/// * **When I am not sure** — the default is in `footing::RULE`, and it is a
///   default somebody may disagree with. Some people want the guess anyway.
/// * **When I think you are wrong** — the default is *once, then do it*. That
///   is a real position and it is not everyone's: it can be zero times, or it
///   can be "argue properly, I want the fight".
/// * **How much to say** — the one thing users complain about first, and the
///   one nothing in this app has ever asked them.
/// * **When to interrupt** — currently never, because nothing can. Writing the
///   answer down *before* Syn can interrupt is the point: it is a limit agreed
///   in advance rather than a setting added after the first annoyance.
///
/// This is the personality setting's replacement, and it replaces it in kind:
/// `personality` picks a voice from three, and none of the three says anything
/// about what Syn *does*. A contract in the user's own words, in a file they
/// can edit and diff, says both.
///
/// The prose is deliberately in the second person and deliberately opinionated
/// — it is a draft to argue with. A template of empty headings is the blank
/// page again with more steps.
pub const TEMPLATE: &str = r#"# How we work together

## When I am not sure
Say so. Do not dress a guess up as a result — I would rather have "I have not
looked" than a confident sentence I have to check.

## When you think I am wrong
Say it once, briefly, with the reason. Then do what I asked anyway. Do not
bring it up again in the same conversation.

## How much to say
The answer first, the reasoning after, and only if I ask. Numbers before
explanations. Do not pad.

## When you may interrupt me
Only when something is actually broken. Not to be helpful.
"#;

/// Everything after any YAML frontmatter.
///
/// Shares the rule with `commands::nodes::frontmatter_end` rather than
/// inventing a second one: a file this app both writes and reads must agree
/// with itself about where the frontmatter ends.
fn body_of(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.first().map(|l| l.trim_end()) != Some("---") {
        return raw.trim().to_string();
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|l| l.trim_end() == "---")
        .map(|p| p + 2)
        .unwrap_or(0);
    lines[end..].join("\n").trim().to_string()
}

/// What the user wrote, or `None` when they have written nothing.
///
/// `None` for a missing file, an unreadable one, and one holding only
/// whitespace or frontmatter — all three mean the same thing to the prompt, and
/// distinguishing them would only produce a heading over nothing.
pub fn load(vault_path: &str) -> Option<String> {
    let raw = std::fs::read_to_string(path(vault_path)).ok()?;
    let body = body_of(&raw);
    (!body.is_empty()).then_some(body)
}

/// The section as the prompt carries it, cut to the budget if it has to be.
///
/// Kept separate from `load` so the untruncated text is what the editor shows:
/// a person editing their own instructions should see all of them, and be told
/// which part Syn is actually getting.
pub fn block(body: &str) -> Option<String> {
    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    let total = body.chars().count();
    if total <= BUDGET_CHARS {
        return Some(format!("{body}\n\n"));
    }

    let shown: String = body.chars().take(BUDGET_CHARS).collect();
    Some(format!(
        "{shown}\n\n(That is the first {BUDGET_CHARS} characters of {total} in the user's \
         `{FILE}`. The rest was not sent — say so if they ask why something in it was not \
         followed.)\n\n",
    ))
}

/// What a chosen voice becomes, in the user's own words.
///
/// `auto` has no line: it is the default, it is now unconditional in
/// `prompt::IDENTITY`, and writing *"adapt to me"* into somebody's contract
/// would be putting words in their mouth for a choice they never made.
fn voice_line(personality: &str) -> Option<&'static str> {
    match personality {
        "casual" => Some(
            "## How to talk to me\nTiếng Việt, xưng tao/mày, thoải mái như bạn thân. Đừng khách sáo.\n",
        ),
        "professional" => Some(
            "## How to talk to me\nTiếng Việt, xưng tôi/bạn, lịch sự và rõ ràng. Trình bày có cấu trúc.\n",
        ),
        _ => None,
    }
}

/// Carry a chosen voice into the file, once, before the setting goes.
///
/// # Why this is not just a deletion
///
/// Somebody who set `personality: casual` made a real choice, and the setting
/// that held it is being removed. Dropping it silently would change how Syn
/// talks to them, on an upgrade, with nothing on any screen saying why. The
/// precedent is `custom_system_prompt`, which was moved rather than deleted for
/// the same reason.
///
/// # Why an existing file wins
///
/// If `SYN.md` is already there, nothing is written. The file is the contract
/// and the contract outranks a setting — and appending to a document somebody
/// wrote, without asking, is exactly the kind of quiet edit this whole module
/// is arranged against. The log line says what happened so it is findable.
///
/// Returns whether anything was written, so the caller clears the setting only
/// once the words are definitely somewhere else.
pub fn migrate_personality(vault_path: &str, personality: &str) -> bool {
    let Some(line) = voice_line(personality) else {
        return false;
    };
    if path(vault_path).exists() {
        log::info!(
            "[Syn] `personality: {personality}` is retired; {FILE} already exists and wins, so \
             nothing was written. Add a line there if the voice matters."
        );
        return false;
    }
    match std::fs::write(path(vault_path), line) {
        Ok(()) => {
            log::info!("[Syn] Moved `personality: {personality}` into {FILE}");
            true
        }
        Err(e) => {
            log::warn!("[Syn] Could not write {FILE}: {e}");
            false
        }
    }
}

/// Move a `custom_system_prompt` into the file, once.
///
/// Runs when there is a setting and no file. Best effort and silent on failure:
/// a vault that cannot be written to is a reason to keep using the setting, not
/// a reason to refuse the message.
///
/// Returns whether it wrote anything, so the caller can clear the setting only
/// when the text is definitely somewhere else.
pub fn migrate(vault_path: &str, from_settings: &str) -> bool {
    let text = from_settings.trim();
    if text.is_empty() || path(vault_path).exists() {
        return false;
    }
    match std::fs::write(path(vault_path), format!("{text}\n")) {
        Ok(()) => {
            log::info!("[Syn] Moved custom_system_prompt into {FILE}");
            true
        }
        Err(e) => {
            log::warn!("[Syn] Could not write {FILE}, keeping the setting: {e}");
            false
        }
    }
}

/// Write what the user typed. An empty body removes the file.
pub fn save(vault_path: &str, body: &str) -> std::io::Result<()> {
    let body = body.trim();
    if body.is_empty() {
        // Removing rather than leaving an empty file, so that "I have no
        // standing instructions" and "I have a file full of nothing" are the
        // same state on disk as they already are in the prompt.
        return match std::fs::remove_file(path(vault_path)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    std::fs::write(path(vault_path), format!("{body}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault() -> tempfile::TempDir {
        tempfile::tempdir().expect("temp vault")
    }

    fn at(dir: &tempfile::TempDir) -> String {
        dir.path().to_str().expect("utf8").to_string()
    }

    #[test]
    fn nothing_written_is_nothing_to_say() {
        let dir = vault();
        assert_eq!(load(&at(&dir)), None);
    }

    #[test]
    fn what_the_user_typed_comes_back() {
        let dir = vault();
        save(&at(&dir), "Luôn trả lời bằng tiếng Việt.").expect("written");
        assert_eq!(load(&at(&dir)).as_deref(), Some("Luôn trả lời bằng tiếng Việt."));
    }

    /// Sync writes `node_id` into the frontmatter of every Markdown file it
    /// touches. Without stripping it, a synced vault would put a uuid at the
    /// top of Syn's standing instructions.
    #[test]
    fn frontmatter_that_sync_added_is_not_part_of_the_instructions() {
        let dir = vault();
        std::fs::write(
            path(&at(&dir)),
            "---\nnode_id: 01J7X\ntitle: SYN\n---\n\nSố trước, lý do sau.\n",
        )
        .expect("written");

        assert_eq!(load(&at(&dir)).as_deref(), Some("Số trước, lý do sau."));
    }

    #[test]
    fn a_file_holding_only_frontmatter_says_nothing() {
        let dir = vault();
        std::fs::write(path(&at(&dir)), "---\nnode_id: 01J7X\n---\n\n   \n").expect("written");
        assert_eq!(load(&at(&dir)), None);
    }

    #[test]
    fn whitespace_is_not_an_instruction() {
        let dir = vault();
        std::fs::write(path(&at(&dir)), "   \n\n\t\n").expect("written");
        assert_eq!(load(&at(&dir)), None);
    }

    /// "No standing instructions" and "a file full of nothing" are the same
    /// state to the prompt, so they are the same state on disk.
    #[test]
    fn saving_nothing_removes_the_file() {
        let dir = vault();
        save(&at(&dir), "something").expect("written");
        assert!(path(&at(&dir)).exists());

        save(&at(&dir), "  ").expect("removed");
        assert!(!path(&at(&dir)).exists());
        // And removing what is not there is not a failure.
        save(&at(&dir), "").expect("still fine");
    }

    #[test]
    fn a_short_body_rides_unchanged() {
        assert_eq!(block("be brief").as_deref(), Some("be brief\n\n"));
        assert_eq!(block("   "), None);
    }

    /// A silently truncated instruction is one the user believes is in force
    /// and is not.
    #[test]
    fn a_long_body_is_cut_and_says_so() {
        let long = "x".repeat(BUDGET_CHARS + 250);
        let block = block(&long).expect("a block");

        assert_eq!(block.matches('x').count(), BUDGET_CHARS, "cut at the budget");
        assert!(block.contains(&format!("of {}", BUDGET_CHARS + 250)), "{block}");
        assert!(block.contains("SYN.md"), "it names the file:\n{block}");
        assert!(
            block.contains("say so if they ask why"),
            "and tells the model to admit it:\n{block}"
        );
    }

    #[test]
    fn a_setting_moves_into_the_file_once() {
        let dir = vault();
        assert!(migrate(&at(&dir), "be brief"));
        assert_eq!(load(&at(&dir)).as_deref(), Some("be brief"));

        // And never again: a file that exists is the user's, whatever an old
        // setting still says.
        assert!(!migrate(&at(&dir), "something else"));
        assert_eq!(load(&at(&dir)).as_deref(), Some("be brief"));
    }

    #[test]
    fn an_empty_setting_writes_no_file() {
        let dir = vault();
        assert!(!migrate(&at(&dir), "   "));
        assert!(!path(&at(&dir)).exists());
    }

    /// The file wins whenever it exists; the setting is only what a vault
    /// written before it still carries.
    ///
    /// Two sources for one thing is how they drift, and this pair would drift
    /// invisibly: the setting is edited in a modal and the file in an editor,
    /// so whichever was touched last is the one the user believes is in force —
    /// with nothing on either screen saying which Syn actually read.
    #[test]
    fn the_file_beats_the_setting_it_replaced() {
        let dir = vault();
        let v = at(&dir);

        assert!(migrate(&v, "old setting"));
        assert_eq!(load(&v).as_deref(), Some("old setting"));

        // The user then edits the file. The stale setting must not win.
        save(&v, "what I actually want").expect("written");
        assert!(!migrate(&v, "old setting"), "never migrates twice");
        assert_eq!(load(&v).as_deref(), Some("what I actually want"));
    }

    /// A choice somebody made must not vanish in an upgrade.
    #[test]
    fn a_chosen_voice_becomes_a_line_they_can_edit() {
        for chosen in ["casual", "professional"] {
            let dir = vault();
            assert!(migrate_personality(&at(&dir), chosen), "{chosen}");
            let body = load(&at(&dir)).expect("written");
            assert!(body.contains("## How to talk to me"), "{chosen}: {body}");
            assert!(body.contains("Tiếng Việt"), "{chosen}: {body}");
        }
    }

    /// `auto` was the default and is now unconditional in the identity section.
    /// Writing "adapt to me" into somebody's contract would be putting words in
    /// their mouth for a choice they never made.
    #[test]
    fn the_default_voice_writes_nothing() {
        let dir = vault();
        assert!(!migrate_personality(&at(&dir), "auto"));
        assert!(!migrate_personality(&at(&dir), "klingon"));
        assert!(!path(&at(&dir)).exists());
    }

    /// The file is the contract, and appending to a document somebody wrote,
    /// without asking, is the quiet edit this module exists to prevent.
    #[test]
    fn an_existing_file_is_never_appended_to() {
        let dir = vault();
        save(&at(&dir), "Số trước, lý do sau.").expect("written");
        assert!(!migrate_personality(&at(&dir), "casual"));
        assert_eq!(load(&at(&dir)).as_deref(), Some("Số trước, lý do sau."));
    }

    /// The template answers the four questions it claims to, and stays inside
    /// the budget it will be read under.
    ///
    /// Checked rather than assumed because it is the one piece of prose in this
    /// app that is offered *as* the user's own words — a heading that quietly
    /// went missing would leave a contract with a hole in it that nobody would
    /// notice, since there is nothing to compare the file against afterwards.
    #[test]
    fn the_template_asks_all_four_questions() {
        for heading in [
            "## When I am not sure",
            "## When you think I am wrong",
            "## How much to say",
            "## When you may interrupt me",
        ] {
            assert!(TEMPLATE.contains(heading), "missing {heading}:\n{TEMPLATE}");
        }
        assert!(TEMPLATE.chars().count() < BUDGET_CHARS, "it fits in one prompt");
        // It rides unchanged, which is what makes "what you see is what Syn
        // gets" true on the first day somebody accepts it.
        assert_eq!(block(TEMPLATE).as_deref(), Some(format!("{}\n\n", TEMPLATE.trim())).as_deref());
    }

    /// Offering it must not be the same as writing it. Nothing here saves.
    #[test]
    fn the_template_is_not_written_to_the_vault_by_anything_here() {
        let dir = vault();
        assert_eq!(load(&at(&dir)), None, "an untouched vault has no SYN.md");
        assert!(!path(&at(&dir)).exists());
    }

    /// The vault root, because a file under `Syn/` is never scanned — and
    /// because this is meant to be found without being told where to look.
    #[test]
    fn it_sits_where_somebody_would_look_for_it() {
        assert_eq!(FILE, "SYN.md");
        assert!(!crate::commands::nodes::is_in_unscanned_dir(FILE));
    }
}
