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
fn write_conversation_file(path: &Path, conv: &ConversationFile) -> AppResult<()> {
    let json = serde_json::to_string_pretty(conv)?;
    atomic_write(path, &json)?;
    Ok(())
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

fn rebuild_index(syn_dir: &Path) -> AppResult<SynIndex> {
    let mut index = SynIndex::default();
    let entries = std::fs::read_dir(syn_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if path.file_name().and_then(|n| n.to_str()) == Some("syn_index.json") {
                continue;
            }
            // Also skip settings.json
            if path.file_name().and_then(|n| n.to_str()) == Some("settings.json") {
                continue;
            }
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
    }
    write_index(syn_dir, &index)?;
    Ok(index)
}

/// List all conversations in the vault's Syn/ directory (metadata only).
/// Returns conversations sorted by `updated_at` descending (newest first).
pub fn list_conversations(vault_path: &str) -> AppResult<Vec<SynConversation>> {
    let syn_dir = ensure_syn_dir(vault_path)?;
    let index = match read_index(&syn_dir) {
        Some(idx) => idx,
        None => rebuild_index(&syn_dir)?,
    };

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
}
