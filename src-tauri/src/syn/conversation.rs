//! Conversation file manager for the Syn (Local AI Chat) feature.
//!
//! Conversations are stored as individual JSON files in `{vault}/Syn/`.
//! Each file contains the full conversation metadata and messages.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::models::syn::{SynConversation, SynConversationFull, SynMessage};

// ═══════════════════════════════════════════════════════════════
//  INTERNAL FILE FORMAT & INDEX
// ═══════════════════════════════════════════════════════════════

/// On-disk JSON format for a Syn conversation file.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct ConversationFile {
    id: String,
    title: String,
    model: Option<String>,
    /// Which provider `model` names. Absent in files written before providers
    /// existed; those are Ollama.
    #[serde(default)]
    provider: Option<crate::models::syn::SynProvider>,
    messages: Vec<SynMessage>,
    created_at: String,
    updated_at: String,
    pinned: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
struct SynIndex {
    conversations: std::collections::HashMap<String, SynConversation>,
}

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

/// Ensure the `Syn/` directory exists inside the vault.
fn ensure_syn_dir(vault_path: &str) -> AppResult<PathBuf> {
    let syn_dir = Path::new(vault_path).join("Syn");
    std::fs::create_dir_all(&syn_dir)
        .map_err(|e| AppError::General(format!("Failed to create Syn directory: {}", e)))?;
    Ok(syn_dir)
}

/// Build the file path for a conversation by ID.
fn conversation_path(syn_dir: &Path, id: &str) -> PathBuf {
    syn_dir.join(format!("{}.json", id))
}

/// Read and deserialize a conversation file.
fn read_conversation_file(path: &Path) -> AppResult<ConversationFile> {
    let content = std::fs::read_to_string(path)?;
    let conv: ConversationFile = serde_json::from_str(&content)?;
    Ok(conv)
}

/// Atomically write content to a file by writing to a temp file first, then renaming.
fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        // Clean up temp file on rename failure
        let _ = std::fs::remove_file(&tmp_path);
        AppError::General(format!("Failed to rename temp file: {}", e))
    })?;
    Ok(())
}

/// Write a conversation file to disk (pretty-printed JSON).
///
/// Through `vault_json`, which keeps the `metadata` the sync layer stamped and
/// sets `metadata.updated_at`: a conflict on one conversation is then settled
/// by which copy was written last, rather than always by the remote one.
fn write_conversation_file(path: &Path, conv: &ConversationFile) -> AppResult<()> {
    crate::syn::vault_json::write(path, conv)
}

/// Convert a ConversationFile to the metadata-only SynConversation.
fn to_metadata(conv: &ConversationFile) -> SynConversation {
    SynConversation {
        id: conv.id.clone(),
        title: conv.title.clone(),
        model: conv.model.clone(),
        provider: conv.provider,
        message_count: conv.messages.len(),
        created_at: conv.created_at.clone(),
        updated_at: conv.updated_at.clone(),
        pinned: conv.pinned,
    }
}

/// Convert a ConversationFile to SynConversationFull (metadata + messages).
fn to_full(conv: ConversationFile) -> SynConversationFull {
    let meta = to_metadata(&conv);
    SynConversationFull {
        meta,
        messages: conv.messages,
    }
}

// ═══════════════════════════════════════════════════════════════
//  PUBLIC API
// ═══════════════════════════════════════════════════════════════

fn index_path(syn_dir: &Path) -> PathBuf {
    syn_dir.join("syn_index.json")
}

fn read_index(syn_dir: &Path) -> Option<SynIndex> {
    let path = index_path(syn_dir);
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(index) = serde_json::from_str(&content) {
                return Some(index);
            }
        }
    }
    None
}

fn write_index(syn_dir: &Path, index: &SynIndex) -> AppResult<()> {
    let path = index_path(syn_dir);
    let json = serde_json::to_string_pretty(index)?;
    atomic_write(&path, &json)?;
    Ok(())
}

/// The id of the conversation this file holds, if it is a conversation file.
///
/// `Syn/` holds more than conversations — `routines.json`, `proposals.json`,
/// `connectors.json`, `calibration.json` and whatever comes next — and naming each
/// one to skip is a list that is always one file short. A conversation is
/// named by the UUID `create_conversation` gave it, and nothing else is.
fn conversation_id_of(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    uuid::Uuid::parse_str(stem).ok()?;
    Some(stem.to_string())
}

/// Every conversation file in `Syn/`, by id.
fn conversation_files(syn_dir: &Path) -> AppResult<Vec<(String, PathBuf)>> {
    Ok(std::fs::read_dir(syn_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| conversation_id_of(&path).map(|id| (id, path)))
        .collect())
}

fn rebuild_index(syn_dir: &Path) -> AppResult<SynIndex> {
    let mut index = SynIndex::default();
    for (_, path) in conversation_files(syn_dir)? {
        match read_conversation_file(&path) {
            Ok(conv) => {
                let meta = to_metadata(&conv);
                index.conversations.insert(meta.id.clone(), meta);
            }
            Err(e) => {
                log::warn!(
                    "[Syn] Skipping corrupt conversation file {:?}: {}",
                    path.file_name(),
                    e
                );
            }
        }
    }
    write_index(syn_dir, &index)?;
    Ok(index)
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The index, brought up to date with the conversation files on disk.
///
/// The index is a cache of what the files say and does not sync (see
/// `sync::utils::is_local_only`) — each device keeps its own. So conversations
/// arrive, change and disappear underneath it by sync, and this is where it
/// catches up: an entry whose file is gone is dropped, and a file the index has
/// not seen, or that was written after the index was, is read again. Only
/// those; a listing does not re-read every conversation.
fn refresh_index(syn_dir: &Path) -> AppResult<SynIndex> {
    let Some(mut index) = read_index(syn_dir) else {
        return rebuild_index(syn_dir);
    };
    let index_written = modified(&index_path(syn_dir));
    let files = conversation_files(syn_dir)?;
    let mut changed = false;

    let present: std::collections::HashSet<&str> = files.iter().map(|(id, _)| id.as_str()).collect();
    let before = index.conversations.len();
    index.conversations.retain(|id, _| present.contains(id.as_str()));
    changed |= index.conversations.len() != before;

    for (id, path) in &files {
        // Same-instant counts as newer: a coarse clock must not hide a change.
        let stale = match (index_written, modified(path)) {
            (Some(index_at), Some(file_at)) => file_at >= index_at,
            _ => true,
        };
        if index.conversations.contains_key(id) && !stale {
            continue;
        }
        // Written back even when nothing listed changed (the sync layer stamping
        // `metadata`, say), so the index is newer than the file afterwards and
        // the next listing does not read it again.
        changed = true;
        match read_conversation_file(path) {
            Ok(conv) => {
                index.conversations.insert(id.clone(), to_metadata(&conv));
            }
            Err(e) => {
                log::warn!("[Syn] Skipping corrupt conversation file {:?}: {}", path.file_name(), e);
                index.conversations.remove(id);
            }
        }
    }

    if changed {
        write_index(syn_dir, &index)?;
    }
    Ok(index)
}

/// List all conversations in the vault's Syn/ directory (metadata only).
/// Returns conversations sorted by `updated_at` descending (newest first).
pub fn list_conversations(vault_path: &str) -> AppResult<Vec<SynConversation>> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let index = refresh_index(&syn_dir)?;

    let mut conversations: Vec<SynConversation> = index.conversations.into_values().collect();

    // Sort by updated_at descending (newest first)
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    Ok(conversations)
}

/// One lock per conversation that has a send in progress or waiting.
type Held = std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>;

static HELD: std::sync::LazyLock<std::sync::Mutex<Held>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Held::new()));

/// One send at a time, per conversation.
///
/// # Why
///
/// A send reads the conversation, drives a run for as long as that takes —
/// seconds, sometimes minutes — and writes the whole file back. Two sends on one
/// conversation inside that window each write what *they* read plus their own
/// turn, and whichever finishes second erases the other's.
///
/// From the app alone that was possible and did not happen, because a person
/// types into one window at a time. A second surface makes it ordinary: the
/// same conversation open on the desktop while a message arrives from a phone.
///
/// So a send reads under this, lets go for the run, and takes it again to put
/// its turn into the file *as it is by then* — see [`place_turn`]. The first
/// version held it across the run as well, which kept every turn and made a
/// long answer hold up every message sent after it: from a phone, a question
/// that took a minute meant a minute before "thanks" could even be read.
///
/// The guard frees the next send when it is dropped. An entry that nothing
/// holds or waits on is swept out on the next call, so the map is only ever as
/// large as the number of conversations busy at once.
pub async fn hold(id: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let lock = {
        let mut held = HELD.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        // The map's own `Arc` is one; a guard or a waiter is another.
        held.retain(|_, lock| std::sync::Arc::strong_count(lock) > 1);
        held.entry(id.to_string()).or_default().clone()
    };
    lock.lock_owned().await
}

/// Put one finished turn into a conversation read back just now.
///
/// Two runs on one conversation can overlap, so what is in the file when an
/// answer is ready may have grown since the question was read: a quick answer
/// asked after a slow one is written first. Each turn goes in whole — question
/// and answer side by side — so the model reading the conversation later never
/// sees two questions in a row and one answer that could be either's.
///
/// `placeholder` is the empty assistant turn a run left when it stopped to ask
/// permission. Carrying that run on answers the question above it, so the
/// answer takes the placeholder's place rather than landing at the bottom,
/// below turns that came after it.
pub fn place_turn(
    messages: &mut Vec<SynMessage>,
    question: Option<SynMessage>,
    answer: SynMessage,
    placeholder: Option<&str>,
) {
    if let Some(at) = placeholder.and_then(|id| messages.iter().position(|m| m.id == id)) {
        messages[at] = answer;
        if let Some(question) = question {
            messages.insert(at, question);
        }
        return;
    }
    messages.extend(question);
    messages.push(answer);
}

/// Load a full conversation (metadata + messages) by ID.
pub fn get_conversation(vault_path: &str, id: &str) -> AppResult<SynConversationFull> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, id);

    if !path.exists() {
        return Err(AppError::General(format!("Conversation not found: {}", id)));
    }

    let conv = read_conversation_file(&path)?;
    Ok(to_full(conv))
}

/// Create a new empty conversation. Returns the metadata.
pub fn create_conversation(vault_path: &str, title: Option<String>) -> AppResult<SynConversation> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    let conv = ConversationFile {
        id: id.clone(),
        title: title.unwrap_or_else(|| "New Conversation".to_string()),
        model: None,
        provider: None,
        messages: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
        pinned: false,
    };

    let path = conversation_path(&syn_dir, &id);
    write_conversation_file(&path, &conv)?;

    let meta = to_metadata(&conv);
    let mut index =
        read_index(&syn_dir).unwrap_or_else(|| rebuild_index(&syn_dir).unwrap_or_default());
    index.conversations.insert(meta.id.clone(), meta.clone());
    if let Err(e) = write_index(&syn_dir, &index) {
        log::warn!("[Syn] Failed to update conversation index: {}", e);
    }

    log::info!("Created new conversation: {}", id);
    Ok(meta)
}

/// Save a full conversation (overwrites the existing file).
pub fn save_conversation(vault_path: &str, conversation: &SynConversationFull) -> AppResult<()> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, &conversation.meta.id);

    let conv = ConversationFile {
        id: conversation.meta.id.clone(),
        title: conversation.meta.title.clone(),
        model: conversation.meta.model.clone(),
        provider: conversation.meta.provider,
        messages: conversation.messages.clone(),
        created_at: conversation.meta.created_at.clone(),
        updated_at: chrono::Utc::now().to_rfc3339(),
        pinned: conversation.meta.pinned,
    };

    write_conversation_file(&path, &conv)?;

    let mut index =
        read_index(&syn_dir).unwrap_or_else(|| rebuild_index(&syn_dir).unwrap_or_default());
    index
        .conversations
        .insert(conv.id.clone(), to_metadata(&conv));
    if let Err(e) = write_index(&syn_dir, &index) {
        log::warn!("[Syn] Failed to update conversation index: {}", e);
    }

    Ok(())
}

/// Delete a conversation by ID.
pub fn delete_conversation(vault_path: &str, id: &str) -> AppResult<()> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, id);

    // Try to delete the file — if it's already gone (e.g., accidentally deleted), that's OK
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    // Always clean up the conversation index entry
    let mut index =
        read_index(&syn_dir).unwrap_or_else(|| rebuild_index(&syn_dir).unwrap_or_default());
    index.conversations.remove(id);
    if let Err(e) = write_index(&syn_dir, &index) {
        log::warn!("[Syn] Failed to update conversation index: {}", e);
    }

    log::info!("Deleted conversation: {}", id);
    Ok(())
}

/// Rename a conversation (update its title).
pub fn rename_conversation(vault_path: &str, id: &str, new_title: &str) -> AppResult<()> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, id);

    if !path.exists() {
        return Err(AppError::General(format!("Conversation not found: {}", id)));
    }

    let mut conv = read_conversation_file(&path)?;
    conv.title = new_title.to_string();
    conv.updated_at = chrono::Utc::now().to_rfc3339();
    write_conversation_file(&path, &conv)?;

    let mut index =
        read_index(&syn_dir).unwrap_or_else(|| rebuild_index(&syn_dir).unwrap_or_default());
    index
        .conversations
        .insert(conv.id.clone(), to_metadata(&conv));
    if let Err(e) = write_index(&syn_dir, &index) {
        log::warn!("[Syn] Failed to update conversation index: {}", e);
    }

    log::info!("Renamed conversation {} to \"{}\"", id, new_title);
    Ok(())
}

/// Auto-generate a title from the first user message.
/// Truncates to ~50 characters, preserving word boundaries.
pub fn auto_title(first_message: &str) -> String {
    let trimmed = first_message.trim();

    // Use char count instead of byte length for Unicode safety
    let char_count = trimmed.chars().count();
    if char_count <= 50 {
        return trimmed.to_string();
    }

    // Collect the first 50 characters safely
    let truncated: String = trimmed.chars().take(50).collect();

    // Find word boundary near the end (space after position 20)
    match truncated.rfind(' ') {
        Some(space_pos) if space_pos > 20 => {
            format!("{}…", &truncated[..space_pos])
        }
        _ => {
            format!("{}…", truncated)
        }
    }
}

/// Toggle pin status of a conversation.
pub fn pin_conversation(vault_path: &str, id: &str, pinned: bool) -> AppResult<()> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, id);
    if !path.exists() {
        return Err(AppError::General(format!("Conversation not found: {}", id)));
    }
    let mut conv = read_conversation_file(&path)?;
    conv.pinned = pinned;
    conv.updated_at = chrono::Utc::now().to_rfc3339();
    write_conversation_file(&path, &conv)?;

    let mut index =
        read_index(&syn_dir).unwrap_or_else(|| rebuild_index(&syn_dir).unwrap_or_default());
    index
        .conversations
        .insert(conv.id.clone(), to_metadata(&conv));
    if let Err(e) = write_index(&syn_dir, &index) {
        log::warn!("[Syn] Failed to update conversation index: {}", e);
    }

    log::info!("Conversation {} pinned={}", id, pinned);
    Ok(())
}

/// Export a conversation as a Markdown string.
pub fn export_conversation_markdown(vault_path: &str, id: &str) -> AppResult<String> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let path = conversation_path(&syn_dir, id);
    if !path.exists() {
        return Err(AppError::General(format!("Conversation not found: {}", id)));
    }
    let conv = read_conversation_file(&path)?;

    let mut md = format!("# {}\n\n", conv.title);
    md.push_str(&format!(
        "*Model: {}*\n",
        conv.model.as_deref().unwrap_or("unknown")
    ));
    md.push_str(&format!("*Created: {}*\n\n", conv.created_at));
    md.push_str("---\n\n");

    for msg in &conv.messages {
        let role_label = match msg.role.as_str() {
            "user" => "**User**",
            "assistant" => "**Syn**",
            "system" => "**System**",
            _ => "**Unknown**",
        };
        md.push_str(&format!("{} ({})\n\n", role_label, msg.timestamp));
        md.push_str(&msg.content);
        md.push_str("\n\n---\n\n");
    }

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(content: &str) -> SynMessage {
        SynMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: content.to_string(),
            model: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tokens: None,
            duration_ms: None,
            sources: None,
            footing: None,
            tool_calls_log: None,
            images: None,
            plan: None,
        }
    }

    /// Two sends on one conversation, overlapping, and both turns survive.
    ///
    /// Each send does what `send_message_inner` does — read the file, spend a
    /// while, write the whole file back — and the pause is long enough for the
    /// other to read the same file in between, were it allowed to. Without the
    /// lock the second write erases the first turn.
    #[tokio::test]
    async fn two_sends_on_one_conversation_both_keep_their_turn() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8").to_string();
        let id = create_conversation(&vault, None).expect("created").id;

        let send = |content: &'static str| {
            let vault = vault.clone();
            let id = id.clone();
            async move {
                let _held = hold(&id).await;
                let mut conversation = get_conversation(&vault, &id).expect("read");
                tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                conversation.messages.push(said(content));
                save_conversation(&vault, &conversation).expect("written");
            }
        };
        tokio::join!(send("từ máy tính"), send("từ điện thoại"));

        let kept: Vec<String> = get_conversation(&vault, &id)
            .expect("read")
            .messages
            .into_iter()
            .map(|m| m.content)
            .collect();
        assert_eq!(kept.len(), 2, "a turn was lost: {kept:?}");
    }

    fn answered(content: &str) -> SynMessage {
        SynMessage { role: "assistant".to_string(), ..said(content) }
    }

    fn contents(messages: &[SynMessage]) -> Vec<(&str, &str)> {
        messages.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect()
    }

    /// A slow question asked first and answered last keeps its answer beside it
    /// — as a pair after the quick one, never split around it.
    #[test]
    fn a_turn_finished_late_goes_in_whole_after_what_was_written_meanwhile() {
        let mut messages = vec![said("nhanh"), answered("xong nhanh")];
        place_turn(&mut messages, Some(said("chậm")), answered("xong chậm"), None);
        assert_eq!(
            contents(&messages),
            [("user", "nhanh"), ("assistant", "xong nhanh"), ("user", "chậm"), ("assistant", "xong chậm")]
        );
    }

    /// Carrying a stopped run on fills in the gap it left, wherever that is now.
    #[test]
    fn a_carried_on_answer_takes_the_place_of_the_turn_that_stopped() {
        let stopped = answered("");
        let placeholder = stopped.id.clone();
        let mut messages = vec![said("xoá note A"), stopped, said("sau đó"), answered("ok")];
        place_turn(&mut messages, None, answered("đã xoá"), Some(&placeholder));
        assert_eq!(
            contents(&messages),
            [("user", "xoá note A"), ("assistant", "đã xoá"), ("user", "sau đó"), ("assistant", "ok")]
        );

        // Deleted from the app meanwhile: the answer still goes somewhere.
        let mut messages = vec![said("xoá note A")];
        place_turn(&mut messages, None, answered("đã xoá"), Some("gone"));
        assert_eq!(contents(&messages), [("user", "xoá note A"), ("assistant", "đã xoá")]);
    }

    /// Different conversations do not wait on each other.
    #[tokio::test]
    async fn a_busy_conversation_does_not_hold_up_another() {
        let _first = hold("conversation-a").await;
        let other = tokio::time::timeout(std::time::Duration::from_millis(200), hold("conversation-b")).await;
        assert!(other.is_ok(), "conversation-b waited on conversation-a");
    }

    #[test]
    fn test_auto_title_short() {
        let title = auto_title("Hello world");
        assert_eq!(title, "Hello world");
    }

    #[test]
    fn test_auto_title_long() {
        let long_msg =
            "This is a very long message that should be truncated to about fifty characters";
        let title = auto_title(long_msg);
        assert!(title.len() <= 55); // 50 + possible "…" character
        assert!(title.ends_with('…'));
    }

    #[test]
    fn test_auto_title_empty() {
        let title = auto_title("   ");
        assert_eq!(title, "");
    }

    #[test]
    fn test_auto_title_unicode() {
        // Vietnamese with diacritics
        let vn =
            "Đây là một tin nhắn rất dài bằng tiếng Việt với nhiều ký tự đặc biệt và dấu thanh";
        let title = auto_title(vn);
        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 55);

        // Emoji
        let emoji = "🎉🎊🎈🎁🎀🎄🎃🎇🎆🎍🎎🎏🎐🎑🎒🎓🎠🎡🎢🎣🎤🎥🎦🎧🎨🎩🎪🎫🎬🎭🎮🎯🎰🎱🎲🎳🎴🎵🎶🎷🎸🎹🎺🎻🎼🎽🎾🎿🏀🏁";
        let title = auto_title(emoji);
        // Should not panic
        assert!(!title.is_empty());
    }

    /// `Syn/` holds more than conversations, and a file that merely parses
    /// close enough is not one. `routines.json` has none of a conversation's
    /// fields and was skipped as corrupt; a file that did have them would have
    /// been listed as a conversation.
    #[test]
    fn the_index_is_built_from_conversation_files_only() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let real = create_conversation(vault, Some("thật".into())).expect("created");

        let syn = dir.path().join("Syn");
        let lookalike = r#"{"id":"routines","title":"x","model":null,"messages":[],"created_at":"a","updated_at":"b","pinned":false}"#;
        for name in ["routines.json", "proposals.json", "connectors.json", "calibration.json", "settings.json"] {
            std::fs::write(syn.join(name), lookalike).expect("written");
        }
        std::fs::write(syn.join("not-a-uuid.json"), lookalike).expect("written");

        let index = rebuild_index(&syn).expect("rebuilt");
        let ids: Vec<&String> = index.conversations.keys().collect();
        assert_eq!(ids, vec![&real.id]);
    }

    /// The index does not sync, so a conversation that arrives by sync, or is
    /// deleted by it, has to show up in the listing from the files alone.
    #[test]
    fn the_listing_catches_up_with_files_sync_brought_or_took() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let kept = create_conversation(vault, Some("ở đây".into())).expect("created");
        let gone = create_conversation(vault, Some("sẽ bị xoá".into())).expect("created");
        assert_eq!(list_conversations(vault).expect("listed").len(), 2);

        // Another device made one, and deleted another; sync wrote the files.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let syn = dir.path().join("Syn");
        let arrived = uuid::Uuid::new_v4().to_string();
        let file = ConversationFile {
            id: arrived.clone(),
            title: "từ điện thoại".into(),
            model: None,
            provider: None,
            messages: vec![said("chào")],
            created_at: "2026-09-27T00:00:00Z".into(),
            updated_at: "2026-09-27T00:00:00Z".into(),
            pinned: false,
        };
        std::fs::write(conversation_path(&syn, &arrived), serde_json::to_string(&file).expect("json"))
            .expect("arrived");
        std::fs::remove_file(conversation_path(&syn, &gone.id)).expect("removed");

        let mut listed: Vec<String> = list_conversations(vault).expect("listed").into_iter().map(|c| c.id).collect();
        listed.sort();
        let mut expected = vec![kept.id.clone(), arrived.clone()];
        expected.sort();
        assert_eq!(listed, expected);

        // A conversation changed by sync is re-read, not served stale.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut renamed = file.clone();
        renamed.title = "đổi tên ở máy kia".into();
        std::fs::write(conversation_path(&syn, &arrived), serde_json::to_string(&renamed).expect("json"))
            .expect("changed");
        let title = list_conversations(vault)
            .expect("listed")
            .into_iter()
            .find(|c| c.id == arrived)
            .expect("still there")
            .title;
        assert_eq!(title, "đổi tên ở máy kia");
    }

    /// The sync layer's stamp on a conversation survives the next save.
    #[test]
    fn a_saved_conversation_keeps_the_sync_metadata() {
        let dir = tempfile::tempdir().expect("temp");
        let vault = dir.path().to_str().expect("utf8");
        let meta = create_conversation(vault, None).expect("created");
        let path = conversation_path(&dir.path().join("Syn"), &meta.id);

        let mut stamped: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        stamped["metadata"]["node_id"] = serde_json::json!("node-7");
        std::fs::write(&path, stamped.to_string()).expect("stamped");

        let mut full = get_conversation(vault, &meta.id).expect("read");
        full.messages.push(said("một câu"));
        save_conversation(vault, &full).expect("saved");

        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        assert_eq!(back["metadata"]["node_id"], "node-7");
        assert!(back["metadata"]["updated_at"].is_string());
        assert_eq!(get_conversation(vault, &meta.id).expect("read").messages.len(), 1);
    }
}
