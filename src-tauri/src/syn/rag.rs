//! RAG (Retrieval-Augmented Generation) pipeline for Syn.
//!
//! Makes Syn context-aware by retrieving relevant vault data before each LLM call.
//! Pipeline: extract terms → FTS5 search → feed articles → finance nodes →
//! graph expansion → dedup/rank → truncate → format context → build system prompt.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::db::DbBridge;
use crate::error::AppResult;
use crate::models::syn::{ContextChunk, RagConfig, RetrievalResult, SynMessage};
use crate::search;

// ═══════════════════════════════════════════════════════════════
//  STOP WORDS (Vietnamese + English)
// ═══════════════════════════════════════════════════════════════

/// Common Vietnamese and English stop words that carry little semantic meaning.
/// These are stripped from the user's message before building search queries.
const STOP_WORDS: &[&str] = &[
    // Vietnamese — pronouns, particles, connectors
    "tao", "mày", "tôi", "bạn", "là", "của", "có", "không", "được", "cho",
    "với", "và", "hoặc", "hay", "từ", "đến", "trong", "ngoài", "trên", "dưới",
    "này", "đó", "kia", "nào", "gì", "sao", "thì", "mà", "nên", "vì",
    "nếu", "đã", "đang", "sẽ", "rồi", "còn", "cũng", "lại", "ra", "vào",
    "lên", "xuống", "đi", "về", "ở", "tại", "theo", "bởi", "do", "hãy",
    "đừng", "chớ", "nhé", "nhỉ", "ạ", "ơi", "vậy", "thế", "rất", "quá",
    "hơn", "nhất", "hết", "xong", "ừ", "ờ", "uh", "nha", "hen", "nghen",
    // Vietnamese — temporal / question words (often too generic for vault search)
    "hôm", "nay", "ngày", "mấy", "bao", "nhiêu", "bây", "giờ", "lúc",
    "khi", "sáng", "chiều", "tối", "đêm", "qua", "mai", "hơm",
    "tuần", "tháng", "năm", "thứ", "mới", "cũ", "trước", "sau",
    // Vietnamese — common verbs too generic for search
    "làm", "biết", "nói", "nghĩ", "muốn", "cần", "phải", "thấy",
    "viết", "đọc", "xem", "nghe", "hỏi", "trả", "lời", "tìm",
    // English
    "the", "is", "a", "an", "in", "on", "at", "to", "for", "and", "or",
    "but", "not", "with", "from", "by", "as", "it", "its", "this", "that",
    "these", "those", "what", "how", "when", "where", "who", "which", "why",
    "can", "could", "would", "should", "will", "shall", "may", "might",
    "do", "does", "did", "am", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "having", "i", "you", "he", "she", "we", "they",
    "me", "him", "her", "us", "them", "my", "your", "his", "our", "their",
    "of", "about", "up", "down", "out", "off", "over", "under", "again",
    "then", "once", "here", "there", "all", "any", "both", "each", "few",
    "more", "most", "some", "such", "no", "nor", "only", "own", "same",
    "so", "than", "too", "very", "just", "also", "if", "else",
    // English — temporal
    "today", "yesterday", "tomorrow", "now", "time", "date", "day",
    "week", "month", "year", "morning", "afternoon", "evening", "night",
    // Common chat filler
    "hey", "hi", "hello", "ok", "okay", "yeah", "yes", "no", "nope",
    "please", "thanks", "thank", "sure", "right", "well", "like",
    "tell", "show", "give", "find", "get", "let", "know", "see",
    "help", "need", "want",
];

/// Minimum BM25 relevance score to include a result in RAG context.
/// Results below this threshold are considered noise and filtered out.
const MIN_RELEVANCE_SCORE: f64 = 1.5;

// ═══════════════════════════════════════════════════════════════
//  A. EXTRACT SEARCH TERMS
// ═══════════════════════════════════════════════════════════════

/// Lazily-initialized stop word set for efficient lookup.
static STOP_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    STOP_WORDS.iter().copied().collect()
});

/// Extract meaningful search keywords from the user's message and recent conversation.
///
/// Removes Vietnamese + English stop words and deduplicates. Also considers
/// up to 2 most recent messages for context continuity (e.g., follow-up questions).
pub fn extract_search_terms(user_message: &str, recent_messages: &[SynMessage]) -> Vec<String> {
    let stop_set = &*STOP_SET;
    let mut seen = HashSet::new();
    let mut terms = Vec::new();

    // Helper: extract meaningful words from a text string
    let extract_words = |text: &str, seen: &mut HashSet<String>, terms: &mut Vec<String>| {
        for word in text.split_whitespace() {
            // Strip punctuation from edges but keep Vietnamese diacritics
            let cleaned: String = word
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_lowercase();

            if cleaned.is_empty() || cleaned.len() < 2 {
                continue;
            }
            if stop_set.contains(cleaned.as_str()) {
                continue;
            }
            if seen.contains(&cleaned) {
                continue;
            }
            seen.insert(cleaned.clone());
            terms.push(cleaned);
        }
    };

    // Primary: extract from user's current message
    extract_words(user_message, &mut seen, &mut terms);

    // Secondary: extract from up to 2 most recent user messages for context continuity
    let recent_user_msgs: Vec<&SynMessage> = recent_messages
        .iter()
        .rev()
        .filter(|m| m.role == "user")
        .take(2)
        .collect();

    for msg in recent_user_msgs {
        // Only add terms from recent messages that are not already present,
        // giving them a lower implicit weight by adding them after primary terms
        extract_words(&msg.content, &mut seen, &mut terms);
    }

    terms
}

/// Filter out Synabit-specific vault terms from search keywords.
/// These are internal concepts (e.g., "note", "task", "event") that are meaningless
/// when searching external content like feed articles or finance records.
fn filter_vault_terms(terms: &[String]) -> Vec<String> {
    const VAULT_TERMS: &[&str] = &[
        "note", "notes", "task", "tasks", "event", "events",
        "person", "people", "contact", "contacts", "quickcap",
        "vault", "synabit", "node", "nodes", "tag", "tags",
        "file", "files", "whiteboard", "linked", "backlink",
    ];
    terms
        .iter()
        .filter(|t| !VAULT_TERMS.contains(&t.as_str()))
        .cloned()
        .collect()
}

// ═══════════════════════════════════════════════════════════════
//  B. RETRIEVE CONTEXT
// ═══════════════════════════════════════════════════════════════

/// Run the full RAG retrieval pipeline:
/// 1. Extract search terms
/// 2. Search main FTS5 index (notes, tasks, events, etc.)
/// 3. Search feed articles FTS5
/// 4. Search finance nodes (direct SQL, excluded from FTS)
/// 5. Fetch full content for top results
/// 6. Expand via knowledge graph (1-hop)
/// 7. Deduplicate, rank, truncate
pub fn retrieve_context(
    db: &DbBridge,
    user_message: &str,
    conversation_messages: &[SynMessage],
    config: &RagConfig,
) -> AppResult<RetrievalResult> {
    if !config.enabled {
        return Ok(RetrievalResult {
            context_chunks: Vec::new(),
            total_tokens_estimate: 0,
            sources: Vec::new(),
        });
    }

    let start = std::time::Instant::now();

    // Step 1: Extract search terms
    let terms = extract_search_terms(user_message, conversation_messages);
    if terms.is_empty() {
        log::info!("[RAG] No meaningful search terms extracted, skipping retrieval");
        return Ok(RetrievalResult {
            context_chunks: Vec::new(),
            total_tokens_estimate: 0,
            sources: Vec::new(),
        });
    }

    log::info!("[RAG] Extracted {} search terms: {:?}", terms.len(), &terms);

    let terms_joined = terms.join(" ");
    let mut all_chunks: Vec<ContextChunk> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Step 2: Search main FTS5 index (notes, tasks, events, quickcaps, files, blocks)
    let parsed_query = search::parse_query(&terms_joined);
    match db.search_fts(&parsed_query, 1, 10) {
        Ok(response) => {
            log::info!(
                "[RAG] FTS5 returned {} results in {}ms",
                response.results.len(),
                response.query_time_ms
            );
            for result in &response.results {
                // Filter out low-relevance results (noise)
                if result.score < MIN_RELEVANCE_SCORE {
                    log::debug!(
                        "[RAG] Skipping low-score result: {} (score: {:.2})",
                        result.title, result.score
                    );
                    continue;
                }
                if seen_ids.contains(&result.id) {
                    continue;
                }
                seen_ids.insert(result.id.clone());

                let metadata = build_metadata_string(&result.item_type, &result.status, &result.date);

                all_chunks.push(ContextChunk {
                    source_id: result.id.clone(),
                    source_type: result.item_type.clone(),
                    title: result.title.clone(),
                    content: result.snippet.clone(),
                    relevance_score: result.score,
                    metadata: if metadata.is_empty() { None } else { Some(metadata) },
                });
            }
        }
        Err(e) => {
            // RAG is best-effort — log and continue
            log::warn!("[RAG] FTS5 search failed: {}", e);
        }
    }

    // Pre-compute filtered terms for feeds and finance (same input, same output)
    let non_vault_terms = filter_vault_terms(&terms);

    // Step 3: Search feed articles (separate FTS5 table)
    // Only search feeds if we have enough specific terms (not just vault-related words)
    if config.include_feeds {
        if non_vault_terms.len() >= 2 {
            let feed_query = non_vault_terms.join(" ");
            let feed_results = db.search_feed_articles_for_rag(&feed_query, 3);
            log::info!("[RAG] Feed articles returned {} results (query: {:?})", feed_results.len(), feed_query);
            for (id, title, summary, published_at) in &feed_results {
                if seen_ids.contains(id) {
                    continue;
                }
                seen_ids.insert(id.clone());

                all_chunks.push(ContextChunk {
                    source_id: id.clone(),
                    source_type: "feed_article".to_string(),
                    title: title.clone(),
                    content: summary.clone(),
                    relevance_score: 3.0, // Lower score for feed articles (prioritize vault content)
                    metadata: Some(format!("published_at:{}", published_at)),
                });
            }
        } else {
            log::info!("[RAG] Skipping feed search — not enough specific terms (got: {:?})", non_vault_terms);
        }
    }

    // Step 4: Search finance nodes (excluded from main FTS, use direct SQL)
    // Only search finance if we have enough specific terms
    if config.include_finance {
        if non_vault_terms.len() >= 2 {
            let finance_results = db.search_finance_nodes_for_rag(&non_vault_terms, 3);
            log::info!("[RAG] Finance nodes returned {} results", finance_results.len());
            for (id, title, content, properties) in &finance_results {
                if seen_ids.contains(id) {
                    continue;
                }
                seen_ids.insert(id.clone());

                all_chunks.push(ContextChunk {
                    source_id: id.clone(),
                    source_type: "finance".to_string(),
                    title: title.clone(),
                    content: content.clone(),
                    relevance_score: 3.0, // Lower score for finance (prioritize vault content)
                    metadata: Some(properties.clone()),
                });
            }
        } else {
            log::info!("[RAG] Skipping finance search — not enough specific terms (got: {:?})", non_vault_terms);
        }
    }

    // Step 5: Fetch full node content for top ~5 results from main search
    // This enriches the snippet-only FTS results with full content
    let top_ids: Vec<String> = all_chunks
        .iter()
        .filter(|c| c.source_type != "feed_article" && c.source_type != "finance")
        .take(5)
        .map(|c| c.source_id.clone())
        .collect();

    for chunk in all_chunks.iter_mut() {
        if !top_ids.contains(&chunk.source_id) {
            continue;
        }
        match db.get_node(&chunk.source_id) {
            Ok(Some(node)) => {
                // Replace snippet with full content (will be truncated later)
                let content_preview: String = node.content.chars().take(1500).collect();
                chunk.content = content_preview;

                // Extract additional metadata from node properties
                if let Some(props) = node.properties.as_object() {
                    let mut meta_parts = Vec::new();
                    for key in &["status", "priority", "due_date", "start_date", "location", "birthday", "amount", "category"] {
                        if let Some(val) = props.get(*key) {
                            if let Some(s) = val.as_str() {
                                if !s.is_empty() {
                                    meta_parts.push(format!("{}:{}", key, s));
                                }
                            } else if let Some(n) = val.as_f64() {
                                meta_parts.push(format!("{}:{}", key, n));
                            }
                        }
                    }
                    // Extract tags
                    if let Some(tags) = props.get("tags").and_then(|t| t.as_array()) {
                        let tag_str: Vec<&str> = tags
                            .iter()
                            .filter_map(|t| t.as_str())
                            .collect();
                        if !tag_str.is_empty() {
                            meta_parts.push(format!("tags:{}", tag_str.join(",")));
                        }
                    }
                    if !meta_parts.is_empty() {
                        chunk.metadata = Some(meta_parts.join("|"));
                    }
                }
            }
            Ok(None) => {} // Node not found (might be a block or deleted)
            Err(e) => {
                log::warn!("[RAG] Failed to fetch node {}: {}", chunk.source_id, e);
            }
        }
    }

    // Step 6: Graph expansion — follow node_edges for top 5 results (1-hop)
    if config.graph_expansion_depth > 0 {
        let expansion_ids: Vec<String> = all_chunks
            .iter()
            .filter(|c| c.source_type != "feed_article" && c.source_type != "finance")
            .take(5)
            .map(|c| c.source_id.clone())
            .collect();

        for source_id in &expansion_ids {
            let related = db.get_related_nodes_for_rag(source_id, 3);
            for (rel_id, rel_title, rel_type) in &related {
                if seen_ids.contains(rel_id) {
                    continue;
                }
                seen_ids.insert(rel_id.clone());

                // Fetch a brief preview of the related node
                let content_preview = match db.get_node(rel_id) {
                    Ok(Some(node)) => node.content.chars().take(500).collect(),
                    _ => String::new(),
                };

                all_chunks.push(ContextChunk {
                    source_id: rel_id.clone(),
                    source_type: rel_type.clone(),
                    title: rel_title.clone(),
                    content: content_preview,
                    relevance_score: 2.0, // Lower score for graph-expanded results
                    metadata: Some(format!("related_to:{}", source_id)),
                });
            }
        }
    }

    // Step 7: Sort by relevance score (descending) and truncate to max context chars
    all_chunks.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal));

    let mut total_chars = 0usize;
    let mut final_chunks: Vec<ContextChunk> = Vec::new();

    for chunk in all_chunks {
        let chunk_size = chunk.title.len() + chunk.content.len() + chunk.metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        if total_chars + chunk_size > config.max_context_chars {
            // Try to fit a truncated version of this chunk
            let remaining = config.max_context_chars.saturating_sub(total_chars);
            if remaining > 100 {
                let truncated_content: String = chunk.content.chars().take(remaining).collect();
                final_chunks.push(ContextChunk {
                    content: truncated_content,
                    ..chunk
                });
            }
            break;
        }
        total_chars += chunk_size;
        final_chunks.push(chunk);
    }

    // Collect source titles for citation
    let sources: Vec<String> = final_chunks
        .iter()
        .map(|c| c.title.clone())
        .collect();

    // Estimate tokens (~4 chars per token)
    let total_tokens_estimate = total_chars / 4;

    let elapsed = start.elapsed().as_millis();
    log::info!(
        "[RAG] Pipeline complete: {} chunks, ~{} tokens, {}ms",
        final_chunks.len(),
        total_tokens_estimate,
        elapsed
    );

    Ok(RetrievalResult {
        context_chunks: final_chunks,
        total_tokens_estimate,
        sources,
    })
}

/// Build a metadata string from common search result fields.
fn build_metadata_string(item_type: &str, status: &Option<String>, date: &str) -> String {
    let mut parts = Vec::new();
    if let Some(s) = status {
        if !s.is_empty() {
            parts.push(format!("status:{}", s));
        }
    }
    if !date.is_empty() {
        parts.push(format!("date:{}", date));
    }
    if !item_type.is_empty() {
        parts.push(format!("type:{}", item_type));
    }
    parts.join("|")
}

// ═══════════════════════════════════════════════════════════════
//  C. FORMAT CONTEXT
// ═══════════════════════════════════════════════════════════════

/// Format retrieved context chunks into a human-readable string organized by type.
///
/// Groups chunks by their source type and formats each with appropriate icons
/// and relevant metadata fields extracted from the metadata string.
pub fn format_context(result: &RetrievalResult) -> String {
    if result.context_chunks.is_empty() {
        return String::new();
    }

    // Group chunks by source type
    let mut groups: HashMap<String, Vec<&ContextChunk>> = HashMap::new();
    for chunk in &result.context_chunks {
        let group_key = normalize_type_group(&chunk.source_type);
        groups.entry(group_key).or_default().push(chunk);
    }

    let mut output = String::new();

    // Render in a consistent order
    let type_order = [
        ("notes", "NOTES", "📝"),
        ("tasks", "TASKS", "☐"),
        ("events", "EVENTS", "📅"),
        ("people", "PEOPLE", "👤"),
        ("quickcaps", "QUICKCAPS", "⚡"),
        ("files", "FILES", "📁"),
        ("feed_articles", "FEED ARTICLES", "📰"),
        ("finance", "FINANCE", "💰"),
        ("other", "RELATED", "🔗"),
    ];

    for (key, label, icon) in &type_order {
        if let Some(chunks) = groups.get(*key) {
            output.push_str(&format!("=== {} ===\n", label));

            for chunk in chunks {
                let meta = parse_metadata(&chunk.metadata);

                match *key {
                    "notes" => {
                        output.push_str(&format!("{} [{}]\n", icon, chunk.title));
                        if let Some(tags) = meta.get("tags") {
                            let tag_str: String = tags
                                .split(',')
                                .map(|t| format!("#{}", t.trim()))
                                .collect::<Vec<_>>()
                                .join(" ");
                            output.push_str(&format!("Tags: {}\n", tag_str));
                        }
                        let preview: String = chunk.content.chars().take(500).collect();
                        if !preview.is_empty() {
                            output.push_str(&format!("Content: {}\n", preview));
                        }
                    }
                    "tasks" => {
                        output.push_str(&format!("{} [{}]", icon, chunk.title));
                        let mut task_meta = Vec::new();
                        if let Some(due) = meta.get("due_date") {
                            task_meta.push(format!("Due: {}", due));
                        }
                        if let Some(priority) = meta.get("priority") {
                            task_meta.push(format!("Priority: {}", priority));
                        }
                        if let Some(status) = meta.get("status") {
                            task_meta.push(format!("Status: {}", status));
                        }
                        if !task_meta.is_empty() {
                            output.push_str(&format!(" — {}", task_meta.join(", ")));
                        }
                        output.push('\n');
                        let preview: String = chunk.content.chars().take(300).collect();
                        if !preview.is_empty() {
                            output.push_str(&format!("Details: {}\n", preview));
                        }
                    }
                    "events" => {
                        output.push_str(&format!("{} [{}]", icon, chunk.title));
                        let mut event_meta = Vec::new();
                        if let Some(start) = meta.get("start_date") {
                            event_meta.push(format!("Start: {}", start));
                        }
                        if let Some(date) = meta.get("date") {
                            if !event_meta.iter().any(|m| m.contains("Start")) {
                                event_meta.push(format!("Date: {}", date));
                            }
                        }
                        if let Some(location) = meta.get("location") {
                            event_meta.push(format!("Location: {}", location));
                        }
                        if !event_meta.is_empty() {
                            output.push_str(&format!(" — {}", event_meta.join(", ")));
                        }
                        output.push('\n');
                    }
                    "people" => {
                        output.push_str(&format!("{} [{}]", icon, chunk.title));
                        if let Some(birthday) = meta.get("birthday") {
                            output.push_str(&format!(" — Birthday: {}", birthday));
                        }
                        output.push('\n');
                        let preview: String = chunk.content.chars().take(300).collect();
                        if !preview.is_empty() {
                            output.push_str(&format!("Info: {}\n", preview));
                        }
                    }
                    "feed_articles" => {
                        output.push_str(&format!("{} [{}]", icon, chunk.title));
                        if let Some(published) = meta.get("published_at") {
                            output.push_str(&format!(" — Published: {}", published));
                        }
                        output.push('\n');
                        let preview: String = chunk.content.chars().take(400).collect();
                        if !preview.is_empty() {
                            output.push_str(&format!("Summary: {}\n", preview));
                        }
                    }
                    "finance" => {
                        output.push_str(&format!("{} [{}]", icon, chunk.title));
                        let mut fin_meta = Vec::new();
                        if let Some(amount) = meta.get("amount") {
                            fin_meta.push(format!("Amount: {} VND", amount));
                        }
                        if let Some(date) = meta.get("date") {
                            fin_meta.push(format!("Date: {}", date));
                        }
                        if let Some(cat) = meta.get("category") {
                            fin_meta.push(format!("Category: {}", cat));
                        }
                        if !fin_meta.is_empty() {
                            output.push_str(&format!(" — {}", fin_meta.join(", ")));
                        }
                        output.push('\n');
                    }
                    _ => {
                        // Generic format for blocks, files, whiteboards, etc.
                        output.push_str(&format!("{} [{}]\n", icon, chunk.title));
                        let preview: String = chunk.content.chars().take(300).collect();
                        if !preview.is_empty() {
                            output.push_str(&format!("Content: {}\n", preview));
                        }
                    }
                }
                output.push_str("---\n");
            }
            output.push('\n');
        }
    }

    output
}

/// Normalize source_type to a display group key.
fn normalize_type_group(source_type: &str) -> String {
    match source_type {
        "note" => "notes".to_string(),
        "task" => "tasks".to_string(),
        "event" => "events".to_string(),
        "person" | "contact" => "people".to_string(),
        "quickcap" => "quickcaps".to_string(),
        "file" => "files".to_string(),
        "feed_article" => "feed_articles".to_string(),
        t if t.starts_with("finance") => "finance".to_string(),
        _ => "other".to_string(),
    }
}

/// Parse the pipe-delimited metadata string into a key-value map.
fn parse_metadata(metadata: &Option<String>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(meta) = metadata {
        for part in meta.split('|') {
            if let Some(pos) = part.find(':') {
                let key = part[..pos].trim().to_string();
                let val = part[pos + 1..].trim().to_string();
                if !key.is_empty() && !val.is_empty() {
                    map.insert(key, val);
                }
            }
        }
    }
    map
}

// ═══════════════════════════════════════════════════════════════
//  D. BUILD SYSTEM PROMPT
// ═══════════════════════════════════════════════════════════════

/// Build the full system prompt including personality, rules, and vault context.
///
/// The `personality` parameter controls the language style:
/// - `"casual"` — Vietnamese casual (tao/mày)
/// - `"professional"` — Vietnamese formal (tôi/bạn)
/// - `"auto"` — Let the model adapt based on user's language
pub fn build_system_prompt(context: &str, personality: &str) -> String {
    let now = chrono::Local::now();
    let current_date = now.format("%Y-%m-%d").to_string();
    let day_of_week = now.format("%A").to_string();

    let personality_instructions = match personality {
        "casual" => {
            "Respond in Vietnamese with a casual, friendly tone. \
             Use informal pronouns (tao/mày) when the user does. \
             Be witty and conversational, like a close friend."
        }
        "professional" => {
            "Respond in Vietnamese with a professional, polite tone. \
             Use formal pronouns (tôi/bạn). \
             Be clear, structured, and respectful."
        }
        _ => {
            // "auto" — adapt to the user's language and style
            "Match the user's language and communication style. \
             If they write in Vietnamese, respond in Vietnamese. \
             If they write in English, respond in English. \
             If they use casual language (tao/mày), be casual back. \
             If they are formal, be formal."
        }
    };

    let context_section = if context.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n=== VAULT CONTEXT ===\n\
             The following is relevant data from the user's Synabit vault. \
             Use this to answer the user's question accurately.\n\n\
             {}\
             === END CONTEXT ===",
            context
        )
    };

    let tool_guidelines = "Tool usage guidelines:\n\
         - You have tools available. USE THEM proactively when the user's request involves \
         searching, finding, listing, querying, creating, or modifying data.\n\
         - For file/image/video/document queries: ALWAYS call search_files. \
         Example: \"tìm ảnh\", \"find images\", \"list PDFs\" → use search_files.\n\
         - For vault content queries about notes/tasks: use search_vault or get_nodes_by_type.\n\
         - For person-related queries: use search_files with the person parameter, \
         or search_vault to find linked content.\n\
         - When the user asks to create, write, or save something: use create_note, create_task, or create_event.\n\
         - When the user asks to mark a task as done/complete: use search_vault to find the task first, then update_task_status.\n\
         - ALWAYS confirm what you created/updated with the result details.\n\
         - Do NOT just reply with text when a tool can provide concrete results.\n\
         - Call tools FIRST, then summarize the results for the user.";

    format!(
        "You are Syn, a personal AI assistant embedded in the Synabit productivity app. \
         Synabit is a second-brain/productivity tool that stores notes, tasks, events, \
         contacts, files, RSS feeds, and financial records.\n\n\
         {}\n\n\
         Key rules:\n\
         - When referencing vault data, cite sources with [[Title]] notation.\n\
         - If information is not in the provided context, say so honestly — do not fabricate.\n\
         - Keep responses concise and actionable.\n\
         - You can see the user's notes, tasks, events, contacts, feeds, and finances.\n\
         - For tasks and events, pay attention to dates, priorities, and statuses.\n\
         - Today's date: {} ({})\n\n\
         {}\n\
         {}",
        personality_instructions, current_date, day_of_week, tool_guidelines, context_section
    )
}

// ═══════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_terms_basic() {
        let terms = extract_search_terms("tìm task về project Synabit", &[]);
        // "tìm" and "về" should be filtered as stop words
        assert!(!terms.contains(&"tìm".to_string()));
        assert!(!terms.contains(&"về".to_string()));
        // Meaningful terms should remain
        assert!(terms.contains(&"task".to_string()));
        assert!(terms.contains(&"project".to_string()));
        assert!(terms.contains(&"synabit".to_string()));
    }

    #[test]
    fn test_extract_search_terms_general_question() {
        // "hôm nay ngày mấy" — all stop words, should return empty
        let terms = extract_search_terms("hôm nay ngày mấy", &[]);
        assert!(terms.is_empty(), "General questions should produce no search terms, got: {:?}", terms);
    }

    #[test]
    fn test_extract_search_terms_removes_stop_words() {
        let terms = extract_search_terms("tao có cái note gì về meeting không", &[]);
        // "tao", "có", "cái", "gì", "về", "không" are stop words
        assert!(!terms.contains(&"tao".to_string()));
        assert!(!terms.contains(&"có".to_string()));
        assert!(!terms.contains(&"không".to_string()));
        // "note" and "meeting" should remain
        assert!(terms.contains(&"note".to_string()));
        assert!(terms.contains(&"meeting".to_string()));
    }

    #[test]
    fn test_extract_search_terms_deduplication() {
        let terms = extract_search_terms("meeting meeting Meeting", &[]);
        let meeting_count = terms.iter().filter(|t| t.as_str() == "meeting").count();
        assert_eq!(meeting_count, 1);
    }

    #[test]
    fn test_extract_search_terms_with_context() {
        let recent = vec![SynMessage {
            id: "1".to_string(),
            role: "user".to_string(),
            content: "deadline sắp tới".to_string(),
            model: None,
            timestamp: "2026-06-11T00:00:00Z".to_string(),
            tokens: None,
            duration_ms: None,
            sources: None,
            tool_calls_log: None,
            images: None,
        }];
        let terms = extract_search_terms("còn task nào nữa", &recent);
        // Should include "deadline" and "sắp" from context
        assert!(terms.contains(&"deadline".to_string()));
    }

    #[test]
    fn test_format_context_empty() {
        let result = RetrievalResult {
            context_chunks: Vec::new(),
            total_tokens_estimate: 0,
            sources: Vec::new(),
        };
        assert!(format_context(&result).is_empty());
    }

    #[test]
    fn test_format_context_mixed_types() {
        let result = RetrievalResult {
            context_chunks: vec![
                ContextChunk {
                    source_id: "1".to_string(),
                    source_type: "note".to_string(),
                    title: "Meeting Notes".to_string(),
                    content: "Discussed Q2 roadmap".to_string(),
                    relevance_score: 10.0,
                    metadata: Some("tags:work,meetings".to_string()),
                },
                ContextChunk {
                    source_id: "2".to_string(),
                    source_type: "task".to_string(),
                    title: "Review PR".to_string(),
                    content: "".to_string(),
                    relevance_score: 8.0,
                    metadata: Some("status:todo|priority:P1|due_date:2026-06-12".to_string()),
                },
            ],
            total_tokens_estimate: 100,
            sources: vec!["Meeting Notes".to_string(), "Review PR".to_string()],
        };
        let formatted = format_context(&result);
        assert!(formatted.contains("=== NOTES ==="));
        assert!(formatted.contains("=== TASKS ==="));
        assert!(formatted.contains("Meeting Notes"));
        assert!(formatted.contains("Review PR"));
        assert!(formatted.contains("#work"));
        assert!(formatted.contains("Priority: P1"));
    }

    #[test]
    fn test_build_system_prompt_casual() {
        let prompt = build_system_prompt("some context", "casual");
        assert!(prompt.contains("Syn"));
        assert!(prompt.contains("tao/mày"));
        assert!(prompt.contains("VAULT CONTEXT"));
        assert!(prompt.contains("some context"));
    }

    #[test]
    fn test_build_system_prompt_auto_no_context() {
        let prompt = build_system_prompt("", "auto");
        assert!(prompt.contains("Syn"));
        assert!(!prompt.contains("VAULT CONTEXT"));
    }

    #[test]
    fn test_parse_metadata() {
        let meta = Some("status:todo|priority:P1|due_date:2026-06-12".to_string());
        let map = parse_metadata(&meta);
        assert_eq!(map.get("status"), Some(&"todo".to_string()));
        assert_eq!(map.get("priority"), Some(&"P1".to_string()));
        assert_eq!(map.get("due_date"), Some(&"2026-06-12".to_string()));
    }

    #[test]
    fn test_normalize_type_group() {
        assert_eq!(normalize_type_group("note"), "notes");
        assert_eq!(normalize_type_group("task"), "tasks");
        assert_eq!(normalize_type_group("finance_transaction"), "finance");
        assert_eq!(normalize_type_group("person"), "people");
        assert_eq!(normalize_type_group("unknown"), "other");
    }
}
