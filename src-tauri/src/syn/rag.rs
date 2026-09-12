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
    "tao",
    "mày",
    "tôi",
    "bạn",
    "là",
    "của",
    "có",
    "không",
    "được",
    "cho",
    "với",
    "và",
    "hoặc",
    "hay",
    "từ",
    "đến",
    "trong",
    "ngoài",
    "trên",
    "dưới",
    "này",
    "đó",
    "kia",
    "nào",
    "gì",
    "sao",
    "thì",
    "mà",
    "nên",
    "vì",
    "nếu",
    "đã",
    "đang",
    "sẽ",
    "rồi",
    "còn",
    "cũng",
    "lại",
    "ra",
    "vào",
    "lên",
    "xuống",
    "đi",
    "về",
    "ở",
    "tại",
    "theo",
    "bởi",
    "do",
    "hãy",
    "đừng",
    "chớ",
    "nhé",
    "nhỉ",
    "ạ",
    "ơi",
    "vậy",
    "thế",
    "rất",
    "quá",
    "hơn",
    "nhất",
    "hết",
    "xong",
    "ừ",
    "ờ",
    "uh",
    "nha",
    "hen",
    "nghen",
    // Vietnamese — temporal / question words (often too generic for vault search)
    "hôm",
    "nay",
    "ngày",
    "mấy",
    "bao",
    "nhiêu",
    "bây",
    "giờ",
    "lúc",
    "khi",
    "sáng",
    "chiều",
    "tối",
    "đêm",
    "qua",
    "mai",
    "hơm",
    "tuần",
    "tháng",
    "năm",
    "thứ",
    "mới",
    "cũ",
    "trước",
    "sau",
    // Vietnamese — common verbs too generic for search
    "làm",
    "biết",
    "nói",
    "nghĩ",
    "muốn",
    "cần",
    "phải",
    "thấy",
    "viết",
    "đọc",
    "xem",
    "nghe",
    "hỏi",
    "trả",
    "lời",
    "tìm",
    // English
    "the",
    "is",
    "a",
    "an",
    "in",
    "on",
    "at",
    "to",
    "for",
    "and",
    "or",
    "but",
    "not",
    "with",
    "from",
    "by",
    "as",
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "what",
    "how",
    "when",
    "where",
    "who",
    "which",
    "why",
    "can",
    "could",
    "would",
    "should",
    "will",
    "shall",
    "may",
    "might",
    "do",
    "does",
    "did",
    "am",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "have",
    "has",
    "had",
    "having",
    "i",
    "you",
    "he",
    "she",
    "we",
    "they",
    "me",
    "him",
    "her",
    "us",
    "them",
    "my",
    "your",
    "his",
    "our",
    "their",
    "of",
    "about",
    "up",
    "down",
    "out",
    "off",
    "over",
    "under",
    "again",
    "then",
    "once",
    "here",
    "there",
    "all",
    "any",
    "both",
    "each",
    "few",
    "more",
    "most",
    "some",
    "such",
    "no",
    "nor",
    "only",
    "own",
    "same",
    "so",
    "than",
    "too",
    "very",
    "just",
    "also",
    "if",
    "else",
    // English — temporal
    "today",
    "yesterday",
    "tomorrow",
    "now",
    "time",
    "date",
    "day",
    "week",
    "month",
    "year",
    "morning",
    "afternoon",
    "evening",
    "night",
    // Common chat filler
    "hey",
    "hi",
    "hello",
    "ok",
    "okay",
    "yeah",
    "yes",
    "no",
    "nope",
    "please",
    "thanks",
    "thank",
    "sure",
    "right",
    "well",
    "like",
    "tell",
    "show",
    "give",
    "find",
    "get",
    "let",
    "know",
    "see",
    "help",
    "need",
    "want",
];

/// Minimum BM25 relevance score to include a result in RAG context.
/// Results below this threshold are considered noise and filtered out.
/// How far below the best hit a result may score and still be kept.
///
/// Replaces a fixed `MIN_RELEVANCE_SCORE` of 1.5. Scores are only meaningful
/// against other scores for the same query, so the cut is relative: keep
/// anything within this fraction of the strongest match, and let a query that
/// found only weak things still return them rather than nothing.
const RELEVANCE_FRACTION: f64 = 0.25;

/// How much of a vault a word may appear in and still be worth searching for.
///
/// # Measured, on the vault that produced the complaint
///
/// A question in Vietnamese survives stopword removal as a row of bare
/// syllables, and a syllable is not a word. *"vẽ thử một hình minh hoạ kiến
/// trúc của splunk"* became nine terms, and this is how much of that vault's
/// 913 documents each one matches:
///
/// | term | documents | share |
/// | --- | ---: | ---: |
/// | vẽ | 342 | 37% |
/// | minh | 313 | 34% |
/// | hoạ | 100 | 11% |
/// | thử | 99 | 11% |
/// | splunk | 85 | 9% |
/// | một | 69 | 8% |
/// | hình | 44 | 5% |
/// | kiến | 25 | 3% |
/// | trúc | 22 | 2% |
///
/// Searched with `match_any`, that is a union of most of the vault, and BM25
/// then ranks a long note carrying five common syllables above a short one
/// carrying the only word that mattered. The answer arrived under ten source
/// chips — *Sao chúng ta lại ngủ?*, *The map is not the territory*, two loose
/// dates — for a question about Splunk, **in a vault with eighty-five documents
/// mentioning Splunk**. Searched for `splunk` alone the same index answers
/// *Cơ chế off auto refresh của splunk dashboard*, *Xây dựng MTPQ cho Splunk*,
/// *Splunk query*.
///
/// A fifth is where the noise sat on that vault: it takes out `vẽ`, `minh`,
/// `công` (44%), `ty` (31%) and `hiểu` (33%) and leaves everything that names
/// a subject. It is a threshold and not a law; what makes it defensible is
/// that it is measured against the vault in front of it rather than a list of
/// words somebody wrote down.
const TOO_COMMON_SHARE: f64 = 0.20;

/// Below this many documents, a share is not evidence.
///
/// A fifth of nine notes is two. Every word in a small vault looks common, and
/// the filter would answer a real question with nothing — which is the one
/// failure worse than answering it with noise.
const ENOUGH_TO_JUDGE: u32 = 50;

/// Keep the words that could tell one note from another.
///
/// # Why the index is asked rather than a list consulted
///
/// Because which words are common is a fact about **this** vault. `công` is in
/// 44% of the one this was measured on; in a vault of English research notes it
/// is in none, and a stopword list carrying it would be throwing away the only
/// term in the question.
///
/// A word matching nothing is kept. It is as discriminating as a word can be —
/// it simply is not here — and dropping it would turn "nothing in the vault is
/// about this" into "search for the rest of the sentence instead", which is
/// exactly the failure above.
pub fn discriminating(db: &DbBridge, terms: &[String]) -> Vec<String> {
    let total = db.indexed_documents().unwrap_or(0);
    if total < ENOUGH_TO_JUDGE {
        return terms.to_vec();
    }

    let ceiling = (total as f64 * TOO_COMMON_SHARE) as u32;
    terms
        .iter()
        .filter(|term| {
            let found = db.documents_containing(term).unwrap_or(0);
            if found > ceiling {
                log::info!(
                    "[RAG] Dropping \"{term}\": in {found} of {total} documents, which tells \
                     nothing apart"
                );
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

/// Whether a word is Vietnamese — which is to say, one syllable of a word.
///
/// `splunk`, `grafana`, `tcb` are names and are searched as they are; `kiến`,
/// `trúc`, `đỉnh` are syllables, and a syllable on its own means too many
/// things.
///
/// A letter outside ASCII settles it. Without one, the word is a syllable if
/// it has the shape of one: `minh`, `trong`, `hoa` are written with no marks
/// at all, and taken for names they decided the search — "minh hoạ" made
/// `minh` a name beside `splunk`. `splunk` cannot be a syllable (no syllable
/// starts `spl`), nor can `tcb` (no vowel) or `grafana` (three of them).
fn is_a_syllable(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic() && !c.is_ascii()) || shaped_like_a_syllable(word)
}

/// A Vietnamese syllable written without its marks: an initial consonant, a
/// run of one to three vowels, a final consonant — every part a real one.
fn shaped_like_a_syllable(word: &str) -> bool {
    const INITIALS: [&str; 26] = [
        "ngh", "ng", "nh", "ch", "gh", "gi", "kh", "ph", "qu", "th", "tr", "b", "c", "d",
        "g", "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
    ];
    const FINALS: [&str; 8] = ["ng", "nh", "ch", "c", "m", "n", "p", "t"];
    const VOWELS: &str = "aeiouy";

    let w = word.to_ascii_lowercase();
    if w.is_empty() || !w.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    let rest = INITIALS
        .iter()
        .find_map(|i| w.strip_prefix(i).filter(|r| r.starts_with(|c| VOWELS.contains(c))))
        .unwrap_or(&w);
    let vowels = rest.chars().take_while(|c| VOWELS.contains(*c)).count();
    if !(1..=3).contains(&vowels) {
        return false;
    }
    let last = &rest[vowels..];
    last.is_empty() || FINALS.contains(&last)
}

/// Adjacent pairs of Vietnamese syllables, in the order they were written.
///
/// Pairs of words that were next to each other, and neither of them a word
/// that tells nothing — a stop word between two syllables ends the pair.
fn syllable_pairs(text: &str) -> Vec<String> {
    let stop_set = &*STOP_SET;
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()).to_lowercase())
        .collect();

    words
        .windows(2)
        .filter(|pair| {
            pair.iter().all(|w| {
                w.chars().count() >= 2 && is_a_syllable(w) && !stop_set.contains(w.as_str())
            })
        })
        .map(|pair| format!("{} {}", pair[0], pair[1]))
        .collect()
}

/// What a question is searched for, as FTS expressions.
///
/// # Why a Vietnamese word is searched as a pair
///
/// Vietnamese is written a syllable at a time, and after stop words are taken
/// out a question is a row of syllables that each mean several things. Measured
/// on the vault that produced the complaint, *"vẽ thử một hình minh hoạ kiến
/// trúc của splunk"*: `kiến` is in 25 documents and `trúc` in 22 — `kiến thức`,
/// `sáng kiến`, `cấu trúc` — while `"kiến trúc"`, the word that was actually
/// asked about, is in 6. Searched a syllable at a time, those two carried the
/// question to notes about knowledge and structure; the ten source chips under
/// the answer included *Sao chúng ta lại ngủ?*
///
/// So, in order:
///
/// 1. **A word that is not a syllable** — `splunk`, `tcb`, `grafana` — is
///    searched on its own, and **if the vault has it, it is the whole
///    search.** Names are what questions are about. Measured on the same
///    vault, pairs searched beside a name still lost to it: BM25 over an `OR`
///    rewards a long note carrying several of the parts, so `"vẽ thử"` and
///    `"thử một"` — *try drawing*, *try one* — filled the list with daily notes
///    and pushed the three notes about Splunk below the cut. Scores from
///    separate searches cannot be compared, so the answer is not to merge two
///    lists but to search for the name alone.
/// 2. **Otherwise syllables are searched in pairs**, as written, and only pairs
///    the vault actually contains.
/// 3. **A lone syllable only when no pair found anything.** "tìm sách" has no
///    pair worth having, and `sách` is still the question.
/// 4. **Anything in more than a fifth of the vault is dropped**, pair or not.
///    See `TOO_COMMON_SHARE`.
///
/// An English question is all names, and is searched as it always was.
///
/// A small vault is left to `discriminating`: with no evidence of which words
/// are common, the question is searched as it was.
pub fn query_parts(
    db: &DbBridge,
    user_message: &str,
    recent_messages: &[SynMessage],
    terms: &[String],
) -> Vec<String> {
    let total = db.indexed_documents().unwrap_or(0);
    if total < ENOUGH_TO_JUDGE {
        return terms.to_vec();
    }
    let ceiling = (total as f64 * TOO_COMMON_SHARE) as u32;
    let found = |part: &str| db.documents_containing(part).unwrap_or(0);

    let (syllables, names): (Vec<String>, Vec<String>) =
        terms.iter().cloned().partition(|t| is_a_syllable(t));

    let mut parts: Vec<String> = discriminating(db, &names);

    // A name the vault has decides the search. See point 1 above.
    if parts.iter().any(|name| found(name) > 0) {
        log::info!("[RAG] Searching for the names {parts:?}");
        return parts;
    }

    // The same sources `extract_search_terms` reads: this message, then the
    // two before it, so a follow-up keeps the subject it is following.
    let recent_user: Vec<&SynMessage> =
        recent_messages.iter().rev().filter(|m| m.role == "user").take(2).collect();
    let mut pairs: Vec<String> = syllable_pairs(user_message);
    for m in recent_user {
        pairs.extend(syllable_pairs(&m.content));
    }
    let mut seen = HashSet::new();
    pairs.retain(|p| seen.insert(p.clone()));

    let useful: Vec<String> = pairs
        .into_iter()
        .filter(|pair| {
            let n = found(pair);
            n > 0 && n <= ceiling
        })
        .map(|pair| format!("\"{pair}\""))
        .collect();

    if useful.is_empty() {
        parts.extend(discriminating(db, &syllables));
    } else {
        parts.extend(useful);
    }

    log::info!("[RAG] Searching for {parts:?}");
    parts
}

/// What a search that matches plain text is given — feed articles, finance.
///
/// Words, narrowed the way the vault's own search is narrowed and no further.
/// `query_parts` writes FTS expressions: quoted pairs, and a single name when
/// the vault knows it. Both are wrong here. `search_feed_articles_for_rag`
/// quotes what it is handed, so `"cổ phiếu"` goes looking for the quote marks;
/// `search_finance_nodes_for_rag` matches with `LIKE`, where they never match
/// at all. And a lone name is one term, which is below the two these searches
/// ask for before they will run — so a question about a stock price skipped
/// the feed search that had eleven articles about stock prices in it.
fn words_for_matching_text(db: &DbBridge, terms: &[String]) -> Vec<String> {
    filter_vault_terms(&discriminating(db, terms))
}

// ═══════════════════════════════════════════════════════════════
//  A. EXTRACT SEARCH TERMS
// ═══════════════════════════════════════════════════════════════

/// Lazily-initialized stop word set for efficient lookup.
static STOP_SET: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| STOP_WORDS.iter().copied().collect());

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
        "note",
        "notes",
        "task",
        "tasks",
        "event",
        "events",
        "person",
        "people",
        "contact",
        "contacts",
        "quickcap",
        "vault",
        "synabit",
        "node",
        "nodes",
        "tag",
        "tags",
        "file",
        "files",
        "whiteboard",
        "linked",
        "backlink",
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

    // What is left after the words that match half the vault are taken out.
    //
    // Retrieval can now come back with nothing, and that is the point: before
    // this, a question whose only real subject was absent from the vault was
    // answered by searching the *rest of the sentence*, and the answer arrived
    // under ten source chips that had nothing to do with it.
    // Two lists out of one question, because two different searches read them.
    //
    // `words` are words: the vault's own FTS is the only search here that
    // understands a quoted pair, and the feed and finance searches below match
    // what they are given as text. Handed `"cổ phiếu"` they look for the quote
    // marks; handed a question whose name the vault knows, they are handed one
    // word and skip themselves for having too little to go on. Neither is what
    // the narrowing was for.
    let words = words_for_matching_text(db, &terms);
    let terms = query_parts(db, user_message, conversation_messages, &terms);
    if terms.is_empty() {
        log::info!("[RAG] Nothing in the question tells one note from another; not retrieving");
        return Ok(RetrievalResult {
            context_chunks: Vec::new(),
            total_tokens_estimate: 0,
            sources: Vec::new(),
        });
    }

    let terms_joined = terms.join(" ");
    let mut all_chunks: Vec<ContextChunk> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    // Step 2: Search main FTS5 index (notes, tasks, events, quickcaps, files, blocks)
    //
    // `match_any`, because these terms are the leftovers of a *question* after
    // stopword removal, not something a person typed into a search box.
    // Requiring all of them requires the asker to have used the note's own
    // vocabulary: measured on a seeded vault, "What did I decide about
    // pricing, and who disagreed?" retrieved nothing from two notes that were
    // entirely about that decision, because neither contains the word
    // "decide".
    let mut parsed_query = search::parse_query(&terms_joined);
    parsed_query.match_any = true;
    match db.search_fts(&parsed_query, 1, 10) {
        Ok(response) => {
            log::info!(
                "[RAG] FTS5 returned {} results in {}ms",
                response.results.len(),
                response.query_time_ms
            );
            // Established once per query, from the best hit.
            let floor = response
                .results
                .iter()
                .map(|r| r.score)
                .fold(f64::NEG_INFINITY, f64::max)
                * RELEVANCE_FRACTION;

            for result in &response.results {
                // Filter out low-relevance results (noise), relative to the
                // best hit for *this* query rather than against a fixed
                // number. A relevance score is not comparable between
                // queries — it moves with how rare the words are and how long
                // the document is — so an absolute floor rejects a good match
                // for a rare word and admits a poor one for a common phrase.
                // The old floor of 1.5 discarded the only note containing
                // "disagreed", which scored 1.34 and was exactly right.
                if result.score < floor {
                    log::debug!(
                        "[RAG] Skipping low-score result: {} (score: {:.2})",
                        result.title,
                        result.score
                    );
                    continue;
                }
                // Filter out internal app data that shouldn't be surfaced as context
                if result.id.starts_with("Syn/")
                    || result.id.starts_with("Messages/")
                    || result.id.contains("/Messages/")
                {
                    continue;
                }
                // A memory is not a note about the topic; it is a standing
                // claim about the person, and it has its own section of the
                // prompt with its own rules for reading it. Arriving here as
                // one more retrieved chunk would strip that framing and let a
                // remembered preference read as evidence from the vault.
                // Matched on the type rather than the folder, because a file
                // moved out of `Memory/` is still a memory.
                if result.item_type == crate::syn::memory::MEMORY_TYPE {
                    continue;
                }
                if seen_ids.contains(&result.id) {
                    continue;
                }
                seen_ids.insert(result.id.clone());

                let metadata =
                    build_metadata_string(&result.item_type, &result.status, &result.date);

                all_chunks.push(ContextChunk {
                    source_id: result.id.clone(),
                    source_type: result.item_type.clone(),
                    title: result.title.clone(),
                    content: result.snippet.clone(),
                    relevance_score: result.score,
                    metadata: if metadata.is_empty() {
                        None
                    } else {
                        Some(metadata)
                    },
                });
            }
        }
        Err(e) => {
            // RAG is best-effort — log and continue
            log::warn!("[RAG] FTS5 search failed: {}", e);
        }
    }

    // Feeds and finance, which read words rather than FTS expressions.
    let non_vault_terms = words;

    // Step 3: Search feed articles (separate FTS5 table)
    // Only search feeds if we have enough specific terms (not just vault-related words)
    if config.include_feeds {
        if non_vault_terms.len() >= 2 {
            let feed_query = non_vault_terms.join(" ");
            let feed_results = db.search_feed_articles_for_rag(&feed_query, 3);
            log::info!(
                "[RAG] Feed articles returned {} results (query: {:?})",
                feed_results.len(),
                feed_query
            );
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
            log::info!(
                "[RAG] Skipping feed search — not enough specific terms (got: {:?})",
                non_vault_terms
            );
        }
    }

    // Step 4: Search finance nodes (excluded from main FTS, use direct SQL)
    // Only search finance if we have enough specific terms
    if config.include_finance {
        if non_vault_terms.len() >= 2 {
            let finance_results = db.search_finance_nodes_for_rag(&non_vault_terms, 3);
            log::info!(
                "[RAG] Finance nodes returned {} results",
                finance_results.len()
            );
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
            log::info!(
                "[RAG] Skipping finance search — not enough specific terms (got: {:?})",
                non_vault_terms
            );
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
                    for key in &[
                        "status",
                        "priority",
                        "due_date",
                        "start_date",
                        "location",
                        "birthday",
                        "amount",
                        "category",
                    ] {
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
                        let tag_str: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
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
    all_chunks.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut total_chars = 0usize;
    let mut final_chunks: Vec<ContextChunk> = Vec::new();

    for chunk in all_chunks {
        let chunk_size = chunk.title.len()
            + chunk.content.len()
            + chunk.metadata.as_ref().map(|m| m.len()).unwrap_or(0);
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

    // Collect source references for citation + navigation
    let sources: Vec<crate::models::syn::SourceRef> = final_chunks
        .iter()
        .map(|c| crate::models::syn::SourceRef {
            id: c.source_id.clone(),
            title: c.title.clone(),
            node_type: c.source_type.clone(),
        })
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

// ═══════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn index(db: &DbBridge, id: &str, title: &str, body: &str) {
        db.upsert_search_entry(
            id, "note", title, "", body, "{}", None, "2026-08-01T00:00:00Z", id,
        );
    }

    /// A vault shaped like the one that produced ten irrelevant chips: many
    /// notes carrying `kiến` and `trúc` in other words, a few about
    /// architecture, and one about Splunk.
    fn syllable_vault() -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("schema");
        for i in 0..40 {
            index(&db, &format!("Notes/k{i}.md"), &format!("Ghi chú {i}"),
                "kiến thức nền tảng và sáng kiến cải tiến quy trình");
        }
        for i in 0..20 {
            index(&db, &format!("Notes/t{i}.md"), &format!("Cấu trúc {i}"),
                "cấu trúc dữ liệu và cấu trúc tổ chức");
        }
        index(&db, "Notes/arch.md", "Kiến trúc hệ thống", "sơ đồ kiến trúc giám sát");
        index(&db, "Notes/splunk.md", "Splunk query", "cách viết splunk query cho dashboard");
        index(&db, "Notes/sach.md", "Sách hay", "danh sách sách nên đọc");
        db
    }

    /// Feeds and finance get words, and enough of them to run.
    ///
    /// They match what they are handed as text: quoted pairs send them looking
    /// for quote marks, and a lone name is below the two terms they ask for
    /// before they will search at all. The day this broke, a question about a
    /// stock price narrowed to `["tcb"]` and the feed search — eleven articles
    /// about stock prices in it — skipped itself.
    #[test]
    fn the_searches_that_match_text_are_given_words() {
        let db = syllable_vault();
        let q = "vẽ thử một hình minh hoạ kiến trúc của splunk";
        let terms = extract_search_terms(q, &[]);

        let words = words_for_matching_text(&db, &terms);
        assert!(words.len() >= 2, "enough to search with: {words:?}");
        assert!(words.contains(&"splunk".to_string()), "{words:?}");
        assert!(words.iter().all(|w| !w.contains('"')), "no FTS quoting: {words:?}");

        let parts = query_parts(&db, q, &[], &terms);
        assert_eq!(parts, vec!["splunk".to_string()], "the vault's own search still narrows");
    }

    #[test]
    fn a_syllable_written_without_marks_is_still_a_syllable() {
        for syllable in ["minh", "trong", "hoa", "nghieng", "gia", "quy", "an", "khoa"] {
            assert!(is_a_syllable(syllable), "`{syllable}`");
        }
        for name in ["splunk", "tcb", "grafana", "datadog", "dashboard", "api", "k8s", "vpb"] {
            assert!(!is_a_syllable(name), "`{name}`");
        }
    }

    /// A name the vault has is what the question is about, and decides the
    /// search. On the real vault, pairs beside it filled the list with daily
    /// notes and pushed the three notes about Splunk below the cut.
    #[test]
    fn a_name_the_vault_has_decides_the_search() {
        let db = syllable_vault();
        let q = "vẽ thử một hình minh hoạ kiến trúc của splunk";
        let parts = query_parts(&db, q, &[], &extract_search_terms(q, &[]));
        assert_eq!(parts, vec!["splunk".to_string()]);

        let found = retrieve_context(&db, q, &[], &RagConfig::default()).expect("retrieval");
        let ids: Vec<&str> = found.sources.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["Notes/splunk.md"], "nothing about knowledge or structure");
    }

    /// Without a name the vault has, `kiến` and `trúc` are searched as the one
    /// word they make — not as the several each of them means.
    #[test]
    fn syllables_are_searched_as_the_word_they_make() {
        let db = syllable_vault();
        let q = "vẽ thử một hình minh hoạ kiến trúc của datadog";
        let parts = query_parts(&db, q, &[], &extract_search_terms(q, &[]));

        assert!(parts.contains(&"\"kiến trúc\"".to_string()), "{parts:?}");
        for alone in ["kiến", "trúc"] {
            assert!(!parts.contains(&alone.to_string()), "`{alone}` alone is not the word: {parts:?}");
        }

        let found = retrieve_context(&db, q, &[], &RagConfig::default()).expect("retrieval");
        let ids: Vec<&str> = found.sources.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&"Notes/arch.md"), "{ids:?}");
        assert!(
            ids.iter().all(|id| !id.starts_with("Notes/k") && !id.starts_with("Notes/t")),
            "nothing about knowledge or structure: {ids:?}"
        );
    }

    /// A one-syllable subject with no pair to make is still the question.
    #[test]
    fn a_lone_syllable_is_searched_when_no_pair_found_anything() {
        let db = syllable_vault();
        let q = "tìm sách";
        let parts = query_parts(&db, q, &[], &extract_search_terms(q, &[]));
        assert_eq!(parts, vec!["sách".to_string()]);
    }

    /// A follow-up keeps the subject it is following.
    #[test]
    fn a_follow_up_keeps_the_name_from_the_question_before() {
        let db = syllable_vault();
        index(&db, "Notes/tcb.md", "TCB", "ghi chép về cổ phiếu tcb");
        let before = vec![SynMessage {
            id: "u0".into(), role: "user".into(), content: "giá cổ phiếu tcb hôm nay".into(),
            model: None, timestamp: String::new(), tokens: None, duration_ms: None,
            sources: None, footing: None, tool_calls_log: None, images: None,
        }];
        let q = "giá này so với đỉnh của 1 năm trở lại đây thì như nào";
        let parts = query_parts(&db, q, &before, &extract_search_terms(q, &before));
        assert!(parts.contains(&"tcb".to_string()), "{parts:?}");
    }

    /// A word that matches half the vault cannot tell one note from another.
    ///
    /// This is the whole of the complaint, in a test. A question in Vietnamese
    /// survives stopword removal as a row of bare syllables — `vẽ`, `minh`,
    /// `công`, `hiểu` — and `match_any` turns those into a union of most of the
    /// vault. The one word that named a subject was then outranked by long
    /// notes carrying five common ones.
    #[test]
    fn a_word_in_half_the_vault_is_not_a_search_term() {
        let db = DbBridge::new_in_memory_full().expect("schema");

        // Sixty documents, because below `ENOUGH_TO_JUDGE` a share is not
        // evidence and the filter stands aside.
        for i in 0..60 {
            db.upsert_search_entry(
                &format!("Notes/{i}.md"),
                "note",
                &format!("Ghi chú {i}"),
                "",
                // In every one of them, and therefore useless.
                "công ty một buổi họp",
                "{}",
                None,
                "2026-08-01T00:00:00Z",
                &format!("Notes/{i}.md"),
            );
        }
        // In one, and therefore the only word worth searching for.
        db.upsert_search_entry(
            "Notes/splunk.md",
            "note",
            "Splunk query",
            "",
            "cách viết splunk query cho dashboard",
            "{}",
            None,
            "2026-08-01T00:00:00Z",
            "Notes/splunk.md",
        );

        let asked: Vec<String> = ["công", "ty", "splunk"].iter().map(|s| s.to_string()).collect();
        assert_eq!(discriminating(&db, &asked), vec!["splunk".to_string()]);
    }

    /// A word the vault has never heard of is the most discriminating word
    /// there is. Dropping it would turn "nothing here is about this" into
    /// "search for the rest of the sentence instead", which is the failure.
    #[test]
    fn a_word_that_matches_nothing_is_kept() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        for i in 0..60 {
            db.upsert_search_entry(
                &format!("Notes/{i}.md"),
                "note",
                "Ghi chú",
                "",
                "công ty một buổi họp",
                "{}",
                None,
                "2026-08-01T00:00:00Z",
                &format!("Notes/{i}.md"),
            );
        }

        let asked = vec!["công".to_string(), "kubernetes".to_string()];
        assert_eq!(discriminating(&db, &asked), vec!["kubernetes".to_string()]);
    }

    /// A fifth of nine notes is two. Every word in a small vault looks common,
    /// and answering a real question with nothing is the one failure worse than
    /// answering it with noise.
    #[test]
    fn a_small_vault_keeps_every_word() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        for i in 0..5 {
            db.upsert_search_entry(
                &format!("Notes/{i}.md"),
                "note",
                "Ghi chú",
                "",
                "công ty một buổi họp",
                "{}",
                None,
                "2026-08-01T00:00:00Z",
                &format!("Notes/{i}.md"),
            );
        }

        let asked = vec!["công".to_string(), "ty".to_string()];
        assert_eq!(discriminating(&db, &asked).len(), 2);
    }

    /// And when nothing survives, nothing is retrieved — rather than the
    /// question being answered by whatever the leftover words happened to hit.
    #[test]
    fn a_question_of_nothing_but_common_words_retrieves_nothing() {
        let db = DbBridge::new_in_memory_full().expect("schema");
        for i in 0..60 {
            db.upsert_search_entry(
                &format!("Notes/{i}.md"),
                "note",
                "Ghi chú",
                "",
                "công ty một buổi họp",
                "{}",
                None,
                "2026-08-01T00:00:00Z",
                &format!("Notes/{i}.md"),
            );
        }

        let found = retrieve_context(
            &db,
            "công ty một buổi họp",
            &[],
            &RagConfig::default(),
        )
        .expect("retrieval");

        assert!(found.sources.is_empty(), "{:?}", found.sources);
        assert!(found.context_chunks.is_empty());
    }

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
        assert!(
            terms.is_empty(),
            "General questions should produce no search terms, got: {:?}",
            terms
        );
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
            footing: None,
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
            sources: vec![
                crate::models::syn::SourceRef {
                    id: "Notes/Meeting Notes.md".to_string(),
                    title: "Meeting Notes".to_string(),
                    node_type: "note".to_string(),
                },
                crate::models::syn::SourceRef {
                    id: "Tasks/Review PR.md".to_string(),
                    title: "Review PR".to_string(),
                    node_type: "task".to_string(),
                },
            ],
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

/// P1.6: does the RAG pipeline still earn its 1,173 lines?
///
/// The pipeline exists to work around a small context window. It extracts
/// keywords, runs FTS5, expands along the graph, dedupes, truncates to
/// `max_context_chars`, and staples the result into the system prompt — all so
/// that a model which cannot go and look is handed something to look at.
///
/// The assistant can now go and look. `query_nodes` reads the same index with
/// the same syntax the app uses, `list_schemas` describes the vault, and the
/// context window is the model's rather than 8,192 tokens. So the question is
/// no longer whether retrieval helps; it is whether *pre-fetched* retrieval
/// still adds anything on top of tools, and what it costs when it does not.
///
/// This measures that. Both arms keep the tools, because production has them:
///
/// - **stuffed** — retrieval runs and its output goes in the system prompt.
///   Today's behaviour.
/// - **agentic** — the system prompt carries no vault context. The model has
///   to search.
///
/// ```bash
/// cargo test --lib rag_vs_agentic -- --ignored --nocapture
/// ```
///
/// Not a pass/fail gate. It prints a table and leaves the decision to a person,
/// because "which answer is better" is not a thing an assertion knows.
#[cfg(test)]
mod rag_vs_agentic {
    use super::*;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use crate::models::syn::SynSettings;
    use crate::syn::engine::SynEngine;
    use crate::syn::provider::ChatProvider;

    /// Model output, with the punctuation a model emits folded to the
    /// punctuation a test was written with.
    ///
    /// Three times now the scorer has been the broken thing rather than the
    /// thing it scores, and each time the number it produced looked fine. This
    /// was the third: the marker list held `n't`, the model wrote `couldn’t`
    /// with U+2019, and three perfectly correct answers —
    ///
    /// > I couldn’t find any notes specifically about the Ha Long trip. The
    /// > only result was [[Hanoi office]], which doesn’t appear related.
    ///
    /// — were recorded as the model failing. Substring matching is cheap and
    /// checkable, which is why it is used, but it is only as good as the
    /// assumption that both sides spell things the same way, and a language
    /// model does not.
    fn normalise(reply: &str) -> String {
        reply
            .to_lowercase()
            // Typographic apostrophes, including the modifier letter form some
            // models emit.
            .replace(['\u{2018}', '\u{2019}', '\u{02bc}'], "'")
            .replace(['\u{201c}', '\u{201d}'], "\"")
            // Non-breaking and other exotic spaces, so a space-padded marker
            // like "not " still matches.
            .replace(['\u{00a0}', '\u{2009}', '\u{202f}'], " ")
    }

    /// One question, and what a correct answer has to contain.
    ///
    /// Substrings rather than a judge model: they are checkable, they are
    /// cheap, and a wrong answer that happens to contain the right string is
    /// visible in the transcript this prints anyway.
    struct Question {
        ask: &'static str,
        /// Every one of these must appear in the reply.
        wants: &'static [&'static str],
        /// At least one of these must appear, when there is more than one way
        /// to say the right thing.
        ///
        /// `wants` requires all of its entries, which is right for a fact — an
        /// answer about the pricing decision has to contain both "per-seat"
        /// and "Mai". It is wrong for an answer whose *shape* is what matters,
        /// and it was wrong in a way that mattered: the honest-no question
        /// asked for the substrings "no" and "not", so
        ///
        /// > Tôi không tìm thấy ghi chú nào cụ thể về chuyến đi Hạ Long của bạn.
        ///
        /// was scored a failure. That answer is entirely correct. This is a
        /// Vietnamese-first app whose default personality adapts to the user's
        /// language, and a measure that only recognises English answers
        /// mismeasures it in exactly the case it exists to test.
        any_of: &'static [&'static str],
        /// None of these may appear. Catches confident invention.
        refuses: &'static [&'static str],
        /// What this question is really testing.
        about: &'static str,
        /// Node ids that carry the answer, for judging retrieval on its own.
        ///
        /// Empty means no node can answer it — either because the answer is a
        /// count that has to be computed, or because the honest answer is that
        /// the vault has nothing. Both are real cases and both are ones where
        /// anything retrieved is at best cost.
        relevant: &'static [&'static str],
        /// Node ids that, if retrieved, actively point the wrong way.
        ///
        /// Not merely irrelevant — irrelevant context is noise, and a model
        /// can ignore noise. These are the ones that *read* like an answer: a
        /// task called "Book the venue" handed to a question about books, or
        /// a note about one place handed to a question about another.
        misleading: &'static [&'static str],
    }

    const QUESTIONS: &[Question] = &[
        Question {
            ask: "What is the wifi password in the Hanoi office?",
            wants: &["ha-noi-2026"],
            refuses: &[],
            about: "one fact in one note — retrieval's best case",
            any_of: &[],
            relevant: &["Notes/Office.md"],
            misleading: &[],
        },
        Question {
            ask: "How many tasks do I have that are not done? Give me the number.",
            wants: &["4"],
            refuses: &["7"],
            about: "a count — stuffed context cannot count, only sample",
            any_of: &[],
            // Nothing can answer this by being read; it has to be counted.
            relevant: &[],
            misleading: &[],
        },
        Question {
            ask: "Which book did I rate highest, and what did I rate it?",
            wants: &["Sapiens", "5"],
            refuses: &[],
            about: "an invented type nothing was written for",
            any_of: &[],
            relevant: &["Books/sapiens.md"],
            // A task, retrieved because its title starts with the word "Book".
            misleading: &["Tasks/d.md"],
        },
        Question {
            ask: "What did I decide about pricing, and who disagreed?",
            wants: &["per-seat", "Mai"],
            refuses: &[],
            about: "two facts in two different notes",
            any_of: &[],
            relevant: &["Notes/Pricing decision.md", "Notes/Pricing pushback.md"],
            misleading: &[],
        },
        Question {
            ask: "Do I have any notes about the Ha Long trip?",
            wants: &[],
            refuses: &["Ha Long Bay hotel"],
            about: "the honest no — invention is the failure mode here",
            // Both languages, because the assistant answers in the one the
            // user wrote in and the seeded question is English while the
            // model may well reply in Vietnamese.
            any_of: &[
                "no ", "not ", "n't", "none", "nothing",
                "không", "chưa", "chẳng",
            ],
            // There is no Ha Long note. The right retrieval is none at all.
            relevant: &[],
            // The Hanoi note, reached because FTS splits `ha-noi-2026` and the
            // question says "Ha". It reads like a hit and is not one.
            misleading: &["Notes/Office.md"],
        },
    ];

    fn seed(vault: &std::path::Path) -> DbBridge {
        let db = DbBridge::new_in_memory_full().expect("schema");

        let write = |rel: &str, node_type: &str, title: &str, body: &str, props: serde_json::Value| {
            let path = vault.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(
                &path,
                crate::commands::nodes::markdown_with_frontmatter(title, node_type, &props, body),
            )
            .expect("write");
            db.upsert_node(&NodeMetadata {
                id: rel.to_string(),
                node_type: node_type.to_string(),
                title: title.to_string(),
                content: body.to_string(),
                properties: props.clone(),
                created_at: "2026-08-01T00:00:00Z".into(),
                updated_at: "2026-08-01T00:00:00Z".into(),
                timestamp: 0,
                blocks: None,
            })
            .expect("upsert");
            // The pipeline being measured reads the search index, so a node
            // that is not indexed would make retrieval look worse than it is.
            let tags = props
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            db.upsert_search_entry(
                rel,
                node_type,
                title,
                &tags,
                body,
                &props.to_string(),
                props.get("status").and_then(|v| v.as_str()),
                "2026-08-01T00:00:00Z",
                rel,
            );
        };

        write(
            "Notes/Office.md",
            "note",
            "Hanoi office",
            "Desk 4B by the window. The wifi password is ha-noi-2026, and it changes every June.",
            serde_json::json!({ "tags": ["office"] }),
        );
        write(
            "Notes/Pricing decision.md",
            "note",
            "Pricing decision",
            "After three rounds we settled on per-seat pricing rather than usage-based. \
             It is easier to explain and the finance team can forecast it.",
            serde_json::json!({ "tags": ["product"] }),
        );
        write(
            "Notes/Pricing pushback.md",
            "note",
            "Pricing pushback",
            "Mai disagreed with the per-seat call. Her argument was that our heaviest \
             accounts are small teams, so seats undercharge exactly the people who cost most.",
            serde_json::json!({ "tags": ["product"] }),
        );
        write("People/Mai.md", "person", "Mai", "Product lead.", serde_json::json!({}));

        for (file, title, rating) in [
            ("Books/sapiens.md", "Sapiens", 5),
            ("Books/dune.md", "Dune", 4),
            ("Books/ubik.md", "Ubik", 3),
        ] {
            write(
                file,
                "book",
                title,
                "",
                serde_json::json!({ "rating": rating, "status": "done" }),
            );
        }

        for (file, title, status) in [
            ("Tasks/a.md", "Renew the domain", "todo"),
            ("Tasks/b.md", "Write the changelog", "todo"),
            ("Tasks/c.md", "Fix the login bug", "in_progress"),
            ("Tasks/d.md", "Book the venue", "backlog"),
            ("Tasks/e.md", "Ship the beta", "done"),
            ("Tasks/f.md", "Archive old logs", "done"),
            ("Tasks/g.md", "Update the deps", "done"),
        ] {
            write(file, "task", title, "", serde_json::json!({ "status": status }));
        }

        db
    }

    struct Outcome {
        passed: bool,
        detail: String,
        tool_calls: usize,
        prompt_chars: usize,
        elapsed_ms: u128,
        reply: String,
        /// How the run ended, and how many rounds it took to get there.
        ///
        /// Without this a failure is unattributable: a model that answered
        /// wrongly and a model that ran out of rounds before it could answer
        /// look identical in the table, and they call for opposite responses.
        /// It matters most for the agentic arm, which has to go and look and
        /// therefore spends rounds the stuffed arm does not — so a ceiling set
        /// for the stuffed arm penalises the other one and reads as the other
        /// one being worse.
        state: crate::syn::run::RunState,
        rounds: u8,
    }

    #[allow(clippy::too_many_arguments)]
    async fn ask(
        provider: Box<dyn ChatProvider>,
        settings: &SynSettings,
        model: &str,
        vault: &std::path::Path,
        db: DbBridge,
        question: &Question,
        stuffed: bool,
    ) -> Outcome {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
            .handle()
            .clone();

        let system_prompt = if stuffed {
            let config = RagConfig {
                enabled: true,
                max_context_chars: settings.max_context_chars,
                include_finance: settings.include_finance,
                include_feeds: settings.include_feeds,
                graph_expansion_depth: settings.graph_expansion_depth,
                };
            let retrieval =
                retrieve_context(&db, question.ask, &[], &config).expect("retrieval runs");
            crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt { context: &format_context(&retrieval), custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS })
            .render()
        } else {
            crate::syn::prompt::PromptPlan::for_chat(crate::syn::prompt::ChatPrompt { context: "", custom: None, skills: None, memory: None, focus: None, thread: None, counted: None, budget_chars: crate::syn::prompt::DEFAULT_BUDGET_CHARS })
            .render()
        };

        let msg = |role: &str, content: String| SynMessage {
            id: role.into(),
            role: role.into(),
            content,
            model: None,
            timestamp: String::new(),
            tokens: None,
            duration_ms: None,
            sources: None,
            footing: None,
            tool_calls_log: None,
            images: None,
        };

        let history = vec![
            msg("system", system_prompt.clone()),
            msg("user", question.ask.to_string()),
        ];

        let engine = SynEngine::new(provider);
        let db_state = std::sync::Mutex::new(db);
        let started = std::time::Instant::now();

        let registry = crate::syn::registry::Registry::for_chat();
        let mut run = crate::syn::run::Run::new(
            question.ask,
            None,
            crate::syn::run::Budget::from_settings(settings),
        );

        let reply = engine
            .drive(
                &mut run,
                crate::syn::engine::DriveRequest {
                    app: &app,
                    message_id: "rag-eval-msg",
                    history: &history,
                    model,
                    temperature: Some(settings.temperature),
                    registry: &registry,
                    db: &db_state,
                    vault_path: vault.to_str().expect("utf8"),
                    num_ctx: settings.num_ctx,
                    max_history: settings.max_history_messages,
                    browser: &crate::syn::browser::Waiting::default(),
                    resume_call: None,
                },
            )
            .await;

        let elapsed_ms = started.elapsed().as_millis();

        let (content, tool_calls) = match reply {
            Ok(m) => (m.content, m.tool_calls_log.map(|l| l.len()).unwrap_or(0)),
            Err(e) => (format!("<error: {e}>"), 0),
        };

        let lowered = normalise(&content);
        let missing: Vec<&str> = question
            .wants
            .iter()
            .copied()
            .filter(|w| !lowered.contains(&w.to_lowercase()))
            .collect();
        let missed_shape = !question.any_of.is_empty()
            && !question
                .any_of
                .iter()
                .any(|w| lowered.contains(&w.to_lowercase()));
        let invented: Vec<&str> = question
            .refuses
            .iter()
            .copied()
            .filter(|w| lowered.contains(&w.to_lowercase()))
            .collect();

        let detail = if !missing.is_empty() {
            format!("missing {missing:?}")
        } else if missed_shape {
            format!("said none of {:?}", question.any_of)
        } else if !invented.is_empty() {
            format!("invented {invented:?}")
        } else {
            String::new()
        };

        Outcome {
            state: run.state,
            rounds: run.spent.iterations,
            passed: missing.is_empty() && !missed_shape && invented.is_empty(),
            detail,
            tool_calls,
            prompt_chars: system_prompt.chars().count(),
            elapsed_ms,
            reply: content,
        }
    }

    /// What retrieval actually finds, with no model involved.
    ///
    /// Split out from the A/B above because the two halves fail differently:
    /// retrieval is deterministic and free, the model is neither. Measuring
    /// them together means a bad retrieval and an unlucky sampling temperature
    /// look identical in the table.
    ///
    /// # What this used to measure, and why that was not enough
    ///
    /// It printed the titles retrieved and counted how many questions came
    /// back with nothing. On that measure the pipeline reads as fixed: nothing
    /// comes back empty any more. But "not empty" and "right" are different
    /// claims, and the gap between them is where the interesting failure now
    /// lives — a question about books that retrieves a task called "Book the
    /// venue", and a question about a place the vault has never heard of that
    /// retrieves a note about a different place. Both are non-empty. Both are
    /// worse than empty, because a model handed a plausible wrong note has no
    /// way to know it is wrong, while a model handed nothing is told to search.
    ///
    /// So it counts three things now: facts found, facts missed, and questions
    /// handed something that points the wrong way.
    ///
    /// Runs offline, so it is a normal test rather than an ignored one.
    #[test]
    fn what_retrieval_finds_for_each_question() {
        let dir = tempfile::tempdir().expect("temp vault");
        let db = seed(dir.path());
        let config = RagConfig::default();

        eprintln!("\n── what retrieval finds, before any model sees it ──────────────────");
        eprintln!(
            "{:<52} {:>4} {:>5} {:>7} {:>6} {:>6}",
            "question", "hit", "miss", "misled", "chars", "best"
        );

        let (mut found, mut wanted, mut misled_questions) = (0usize, 0usize, 0usize);

        for q in QUESTIONS {
            let result = retrieve_context(&db, q.ask, &[], &config).expect("retrieval runs");
            let ids: Vec<&str> = result
                .context_chunks
                .iter()
                .map(|c| c.source_id.as_str())
                .collect();

            let hit = q.relevant.iter().filter(|r| ids.contains(r)).count();
            let miss = q.relevant.len() - hit;
            let misled = q.misleading.iter().filter(|m| ids.contains(m)).count();

            found += hit;
            wanted += q.relevant.len();
            if misled > 0 {
                misled_questions += 1;
            }

            let chars: usize = result.context_chunks.iter().map(|c| c.content.len()).sum();
            let best = result
                .context_chunks
                .iter()
                .map(|c| c.relevance_score)
                .fold(0.0_f64, f64::max);

            let unanswerable = q.relevant.is_empty();
            eprintln!(
                "{:<52} {:>4} {:>5} {:>7} {:>6} {:>6.2}",
                q.ask.chars().take(52).collect::<String>(),
                if unanswerable { "–".to_string() } else { hit.to_string() },
                if unanswerable { "–".to_string() } else { miss.to_string() },
                misled,
                chars,
                best,
            );

            // The titles, because a number does not show you what went wrong.
            for chunk in &result.context_chunks {
                let mark = if q.misleading.contains(&chunk.source_id.as_str()) {
                    "✗"
                } else if q.relevant.contains(&chunk.source_id.as_str()) {
                    "✓"
                } else {
                    "·"
                };
                eprintln!(
                    "      {mark} {:<40} {:>6.2}  {}",
                    chunk.title.chars().take(40).collect::<String>(),
                    chunk.relevance_score,
                    chunk.source_id,
                );
            }
        }

        eprintln!(
            "── {found}/{wanted} facts found · {misled_questions}/{} questions given \
             misleading context ──\n",
            QUESTIONS.len()
        );

        // Deliberately not an assertion on the numbers. This prints a table for
        // a person to read and decide from, and pinning today's figures would
        // turn every retrieval change into a failing test that has to be
        // re-blessed rather than read.
    }

    /// The one thing retrieval must never do: answer a question about something
    /// the vault does not contain.
    ///
    /// Separated from the table above because it *is* a rule rather than a
    /// measurement. Everything else about retrieval is a trade — more recall
    /// against more noise — and a model told the context is a sample can work
    /// around noise. This one is not a trade. Asked about a trip that was never
    /// taken, a pipeline that hands back a confident-looking note about
    /// somewhere else has manufactured evidence, and nothing downstream can
    /// tell that it did.
    ///
    /// **This does not pass today.** "Reykjavik supplier" retrieves nothing, as
    /// it should; "Ha Long trip" retrieves the Hanoi office note, because FTS5
    /// tokenises `ha-noi-2026` into `ha`/`noi`/`2026` and the question contains
    /// the word "Ha". Under `match_any` one matched term is enough to surface a
    /// document.
    ///
    /// It is `#[ignore]`d rather than deleted or weakened because it states the
    /// rule correctly and the rule is not held. Both obvious fixes were tried
    /// and measured, and both cost more than they buy — see
    /// `how_much_of_the_question_each_hit_matched` for the numbers and
    /// `docs/adr-rag-vs-agentic-2026-09-03.md` for why nothing was tuned on
    /// five questions from one seeded vault.
    ///
    /// ```bash
    /// cargo test --lib a_question_about_something_absent -- --ignored
    /// ```
    #[test]
    #[ignore = "a known defect, stated as the rule it breaks; see the RAG ADR"]
    fn a_question_about_something_absent_retrieves_nothing() {
        let dir = tempfile::tempdir().expect("temp vault");
        let db = seed(dir.path());
        let config = RagConfig::default();

        for absent in [
            "Do I have any notes about the Ha Long trip?",
            "What did we agree with the Reykjavik supplier?",
        ] {
            let result = retrieve_context(&db, absent, &[], &config).expect("retrieval runs");
            let titles: Vec<&str> = result
                .context_chunks
                .iter()
                .map(|c| c.title.as_str())
                .collect();
            assert!(
                result.context_chunks.is_empty(),
                "`{absent}` has no answer in this vault, and retrieval offered {titles:?}"
            );
        }
    }

    /// How many of the query's terms a result actually matched.
    ///
    /// FTS5's `snippet()` wraps each matched term in `<mark>`, so this is a count
    /// FTS has already done and thrown away. Title as well as snippet, because the
    /// snippet only covers the body column and a match on the title would
    /// otherwise read as no match at all.
    ///
    /// Distinct terms rather than occurrences: a note that says "pricing" nine
    /// times has answered one word of the question, not nine.
    fn marked_terms(snippet: &str, title: &str, terms: &[String]) -> usize {
        let marked: String = snippet.to_lowercase();
        let title = title.to_lowercase();
        terms
            .iter()
            .filter(|term| {
                let needle = term.to_lowercase();
                marked.contains(&format!("<mark>{needle}</mark>")) || title.contains(&needle)
            })
            .count()
    }

    /// The scorer, on answers in both languages.
    ///
    /// It got this wrong once, and the failure was invisible: a correct
    /// Vietnamese answer to the honest-no question was recorded as a failure of
    /// the *model*, and went into a table that a decision was going to be read
    /// off. A measure that is wrong in one language is worse than no measure,
    /// because it is wrong with a number attached.
    #[test]
    fn the_honest_no_is_recognised_in_either_language() {
        let question = QUESTIONS
            .iter()
            .find(|q| q.ask.contains("Ha Long"))
            .expect("the honest-no question is still there");

        let scores = |reply: &str| -> bool {
            let lowered = normalise(reply);
            let missing = question
                .wants
                .iter()
                .any(|w| !lowered.contains(&w.to_lowercase()));
            let missed_shape = !question.any_of.is_empty()
                && !question
                    .any_of
                    .iter()
                    .any(|w| lowered.contains(&w.to_lowercase()));
            let invented = question
                .refuses
                .iter()
                .any(|w| lowered.contains(&w.to_lowercase()));
            !missing && !missed_shape && !invented
        };

        // The answer that was scored a failure, verbatim from the gemma4:e4b run.
        assert!(
            scores(
                "Tôi không tìm thấy ghi chú nào cụ thể về chuyến đi Hạ Long của bạn. \
                 Kết quả tìm kiếm chỉ trả về một ghi chú khác là [[Hanoi office]]."
            ),
            "a correct Vietnamese answer must score as correct"
        );
        assert!(scores("I could not find any notes about a Ha Long trip."));

        // Verbatim from the gpt-5.6-luna run, typographic apostrophe included.
        // All three of these were scored as the model failing.
        assert!(
            scores(
                "I couldn\u{2019}t find any notes specifically about the Ha Long trip. \
                 The only result was [[Hanoi office]], which doesn\u{2019}t appear related."
            ),
            "a curly apostrophe is still an apostrophe"
        );
        assert!(scores(
            "I couldn\u{2019}t find a note specifically about a Ha Long trip."
        ));
        assert!(scores("Bạn chưa có ghi chú nào về Hạ Long."));
        assert!(scores("There's nothing in your vault about that trip."));

        // And it still catches the failure it exists for: inventing the note.
        assert!(
            !scores("Yes — you have a note called Ha Long Bay hotel with the booking."),
            "invention must still fail"
        );

        // The trap the previous check fell into, kept here so it cannot come
        // back. It asked for the substrings "no" and "not", and the word
        // "notes" contains both — in a question whose own text is "Do I have
        // any *notes* about the Ha Long trip?". So any reply that used the word
        // "notes" at all was scored correct, whatever it went on to claim. The
        // check was very nearly vacuous, and it was vacuous in the direction
        // that flatters the thing being measured.
        assert!(
            !scores("Yes, you have notes covering that trip in your vault."),
            "a reply that merely says `notes` must not count as an honest no — this is the \
             substring trap the earlier version of this check fell into"
        );
    }

    /// What the assistant's own search tool finds, for the queries a model
    /// actually writes.
    ///
    /// Retrieval sets `match_any` so that a *question* does not have to use a
    /// note's own vocabulary — the doc comment on that line names this exact
    /// question as the reason. `tool_query_nodes` called `parse_query` and did
    /// not, so the assistant searched with `AND` while the pipeline beside it
    /// searched with `OR`.
    ///
    /// That asymmetry is invisible from either side and shows up as the model
    /// being bad at searching. It cost a real failure in the A/B: asked what
    /// was decided about pricing and who disagreed, the agentic arm searched
    /// twice and answered "I couldn't find any vault notes mentioning
    /// pricing", from a vault holding two notes that are entirely about it.
    ///
    /// Goes through `execute_tool` rather than the query engine underneath it,
    /// because the fallback lives in the tool and a test that reaches past it
    /// would pass while the assistant still failed. That is not hypothetical —
    /// the first version of this test did exactly that.
    #[test]
    fn the_assistants_own_search_finds_what_retrieval_finds() {
        let dir = tempfile::tempdir().expect("temp vault");
        let db: crate::db::DbState = std::sync::Mutex::new(seed(dir.path()));
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
            .handle()
            .clone();
        let ctx = crate::syn::tools::ToolContext {
            db: &db,
            vault_path: dir.path().to_str().expect("utf8"),
            app: &app,
            run_id: None,
        };

        let ask = |query: &str| -> (u64, bool) {
            let out = crate::syn::tools::execute_tool(
                &ctx,
                "query_nodes",
                &serde_json::json!({ "query": query }),
            )
            .expect("the tool runs");
            let parsed: serde_json::Value =
                serde_json::from_str(&out).expect("the tool returns JSON");
            (
                parsed["total_matches"].as_u64().unwrap_or(0),
                parsed["matched_any_word"].as_bool().unwrap_or(false),
            )
        };

        // Queries a model plausibly writes for "What did I decide about
        // pricing, and who disagreed?" — the words of the question, in the
        // orders a model tends to put them.
        let attempts = [
            "pricing",
            "pricing decision",
            "pricing disagreed",
            "decide pricing disagreed",
            "pricing decision disagreed",
        ];

        eprintln!("\n── what `query_nodes` hands the assistant ──");
        eprintln!("{:<34} {:>8}  {}", "query", "matches", "widened");

        let mut empty = Vec::new();
        for attempt in attempts {
            let (total, widened) = ask(attempt);
            eprintln!("{attempt:<34} {total:>8}  {}", if widened { "yes" } else { "" });
            if total == 0 {
                empty.push(attempt);
            }
        }
        eprintln!();

        assert!(
            empty.is_empty(),
            "the assistant's search returns nothing for {empty:?}, in a vault holding two \
             notes about pricing"
        );

        // The precision that already worked must not have been traded away.
        // A query every word of which matches is answered strictly, and is not
        // widened — otherwise this fix would quietly turn every search into OR.
        let (exact, widened) = ask("pricing decision");
        assert_eq!(exact, 1, "an exact match should still be exact");
        assert!(!widened, "a query that found something must not be widened");
    }

    /// What the widening fallback costs when the vault genuinely has nothing.
    ///
    /// The fallback exists because zero results read to a model as an empty
    /// vault. But the same reasoning cuts the other way: a question about
    /// something absent *should* return zero, and widening it turns an honest
    /// nothing into a plausible something. That is the failure retrieval
    /// already has — see `a_question_about_something_absent_retrieves_nothing`
    /// — and the fix must not import it into the tool the assistant uses.
    ///
    /// This measures the cost rather than assuming it either way.
    #[test]
    fn what_the_widening_costs_on_a_question_with_no_answer() {
        let dir = tempfile::tempdir().expect("temp vault");
        let db: crate::db::DbState = std::sync::Mutex::new(seed(dir.path()));
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app")
            .handle()
            .clone();
        let ctx = crate::syn::tools::ToolContext {
            db: &db,
            vault_path: dir.path().to_str().expect("utf8"),
            app: &app,
            run_id: None,
        };

        eprintln!("\n── widening, on questions the vault cannot answer ──");
        for query in [
            "ha long trip",
            "notes ha long trip",
            "reykjavik supplier agreement",
            "dentist appointment",
            // The word for the thing, rather than a word in the thing. A model
            // writing "notes about X" puts this in every query it makes.
            "notes",
            "note",
            "tasks",
        ] {
            let out = crate::syn::tools::execute_tool(
                &ctx,
                "query_nodes",
                &serde_json::json!({ "query": query }),
            )
            .expect("the tool runs");
            let parsed: serde_json::Value = serde_json::from_str(&out).expect("JSON");
            let titles: Vec<String> = parsed["results"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .map(|r| r["title"].as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            eprintln!(
                "  {query:<32} {:>3} match(es)  widened={:<5} {titles:?}",
                parsed["total_matches"].as_u64().unwrap_or(0),
                parsed["matched_any_word"].as_bool().unwrap_or(false),
            );
        }
        eprintln!();
    }

    /// How much of each question a hit actually matched.
    ///
    /// The table above shows that a single number cannot separate the good
    /// hits from the bad ones: the correct answers score 11.65, 4.14, 2.87 and
    /// — for a one-word question — 1.34, while the two misleading ones score
    /// 1.71 and 1.63. Any absolute floor that keeps 1.34 also keeps 1.71. That
    /// is not a badly chosen number; it is the wrong instrument, and both the
    /// fixed floor of 1.5 and the relative floor that replaced it are the same
    /// instrument pointed in opposite directions.
    ///
    /// This measures a different quantity. FTS5's `snippet()` wraps every term
    /// it matched in `<mark>`, so the number of distinct marked terms is how
    /// much of the question a result actually answered — free, exact, and
    /// already being returned. The hypothesis is that the good hits match
    /// several of the question's words and the misleading ones match one.
    ///
    /// # The hypothesis is wrong, and this is the table that says so
    ///
    /// | hit | terms | score | matched | verdict |
    /// | --- | --- | --- | --- | --- |
    /// | Hanoi office (wifi) | 4 | 11.65 | 4 | right |
    /// | Pricing pushback | 3 | 4.14 | 2 | right |
    /// | **Pricing decision** | 3 | 2.87 | **1** | **right** |
    /// | Book the venue | 3 | 1.71 | 1 | wrong |
    /// | Hanoi office (Ha Long) | 4 | 1.63 | 1 | wrong |
    ///
    /// "Pricing decision" answers half of its question — it is the note that
    /// contains "per-seat" — and it matched exactly one of the three terms, the
    /// same as both wrong answers. So a coverage floor of two terms would drop
    /// a correct hit to remove two incorrect ones, on a question the pipeline
    /// currently gets entirely right. Measured, not argued.
    ///
    /// Which leaves both instruments rejected for the same reason: on this
    /// evidence, a scalar cannot tell a weak-but-right hit from a weak-and-wrong
    /// one, whether the scalar is a score or a count. The two failures do not
    /// even share a cause — "Book the venue" is a real lexical match on a real
    /// word, while "ha" is a fragment of a hyphenated token — so one filter was
    /// never going to catch both.
    #[test]
    fn how_much_of_the_question_each_hit_matched() {
        let dir = tempfile::tempdir().expect("temp vault");
        let db = seed(dir.path());

        eprintln!("\n── coverage: how many of the question's words each hit matched ──");
        eprintln!("{:<44} {:>5} {:>6} {:>8}", "hit", "terms", "score", "matched");

        for q in QUESTIONS {
            let terms = extract_search_terms(q.ask, &[]);
            let mut parsed = search::parse_query(&terms.join(" "));
            parsed.match_any = true;

            eprintln!("\n  {}", q.ask);
            let Ok(response) = db.search_fts(&parsed, 1, 10) else {
                continue;
            };
            for r in &response.results {
                let matched = marked_terms(&r.snippet, &r.title, &terms);
                let verdict = if q.misleading.contains(&r.id.as_str()) {
                    "✗"
                } else if q.relevant.contains(&r.id.as_str()) {
                    "✓"
                } else {
                    "·"
                };
                eprintln!(
                    "  {verdict} {:<42} {:>5} {:>6.2} {:>8}",
                    r.title.chars().take(42).collect::<String>(),
                    terms.len(),
                    r.score,
                    matched,
                );
            }
        }
        eprintln!();
    }

    /// Why retrieval comes back empty, measured rather than guessed.
    ///
    /// Two filters sit between a question and the vault, and each one alone
    /// would explain an empty result:
    ///
    /// 1. `extract_search_terms` strips stopwords and hands the rest to the
    ///    parser the *search box* uses, and `build_fts_match` joins terms with
    ///    `AND`. Right for a search box — someone typing "meeting notes" wants
    ///    both — and wrong for a question, which is not a bag of words that all
    ///    appear in one note.
    /// 2. `MIN_RELEVANCE_SCORE` discards anything scoring under 1.5.
    ///
    /// This prints what each costs. Offline, so it is free to re-run after any
    /// change to either.
    #[test]
    fn where_the_recall_goes() {
        let vault_dir = tempfile::tempdir().expect("temp vault");
        let db = seed(vault_dir.path());
        let config = RagConfig {
            enabled: true,
            max_context_chars: 12000,
            include_finance: true,
            include_feeds: true,
            graph_expansion_depth: 1,
            };

        // What the pipeline returns, after both filters.
        let after_filters =
            |q: &str| retrieve_context(&db, q, &[], &config).expect("runs").context_chunks.len();

        // What the index would return, before the relevance floor. Same terms,
        // same query, so the difference is the floor and nothing else.
        let raw = |q: &str| {
            let terms = extract_search_terms(q, &[]);
            let mut parsed = search::parse_query(&terms.join(" "));
            parsed.match_any = true;
            db.search_fts(&parsed, 1, 10)
                .map(|r| {
                    let best = r
                        .results
                        .iter()
                        .map(|x| x.score)
                        .fold(f64::NEG_INFINITY, f64::max);
                    (r.results.len(), if r.results.is_empty() { 0.0 } else { best })
                })
                .unwrap_or((0, 0.0))
        };

        eprintln!("\n── where the recall goes ─────────────────────────────────────────");
        eprintln!("{:<52} {:>6} {:>6} {:>7}", "query", "index", "kept", "best");
        for q in [
            "pricing",
            "disagreed",
            "pricing disagreed",
            "decide pricing disagreed",
            "What did I decide about pricing, and who disagreed?",
            "wifi",
            "What is the wifi password in the Hanoi office?",
        ] {
            let (found, best) = raw(q);
            eprintln!(
                "{:<52} {:>6} {:>6} {:>7.2}",
                q.chars().take(50).collect::<String>(),
                found,
                after_filters(q),
                best
            );
        }
        eprintln!(
            "── `index` is what FTS returned, `kept` is what survived the relative floor ──\n"
        );
    }

    #[tokio::test]
    #[ignore = "spends real API credit and needs a network; run by hand"]
    async fn stuffed_context_versus_letting_it_search() {
        let vault_dir = tempfile::tempdir().expect("temp vault");
        let vault = vault_dir.path();

        let settings = crate::syn::settings::load_settings(
            &std::env::var("SYN_EVAL_VAULT").unwrap_or_else(|_| {
                format!("{}/Documents/vault", std::env::var("HOME").unwrap_or_default())
            }),
        )
        .expect("the real Syn settings");
        let model = settings
            .default_model
            .clone()
            .expect("a default model must be configured");

        let build_provider = || -> Box<dyn ChatProvider> {
            crate::syn::provider::for_settings(
                &settings,
                crate::secrets::SecretManager::get_syn_api_key(None, settings.provider.key_slot()),
            )
        };

        // One run of five questions is not enough to compare accuracy: the
        // agentic arm scored 3, 4 and 5 out of 5 on three consecutive runs of
        // the same build. Cost is stable and accuracy is not, so anyone
        // drawing a conclusion about the latter should raise this first.
        let trials: usize = std::env::var("SYN_EVAL_TRIALS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        eprintln!("\n═══ RAG vs agentic search ═══════════════════════");
        eprintln!("provider {:?}   model {model}", settings.provider);
        eprintln!("both arms keep the tools; only the system prompt differs");
        eprintln!("{trials} trial(s) per question — set SYN_EVAL_TRIALS to raise it\n");

        // passed, calls, ms, prompt chars, runs cut short by the ceiling
        let mut totals = [(0usize, 0usize, 0u128, 0usize, 0usize); 2];

        for q in QUESTIONS {
            eprintln!("── {}", q.ask);
            eprintln!("   ({})", q.about);

            for (i, stuffed) in [true, false].into_iter().enumerate() {
                let mut passes = 0usize;
                for trial in 0..trials {
                    let out = ask(
                        build_provider(),
                        &settings,
                        &model,
                        vault,
                        seed(vault),
                        q,
                        stuffed,
                    )
                    .await;

                    passes += usize::from(out.passed);
                    // A run that ran out of rounds is marked, because a
                    // failure caused by the ceiling is not a failure of the
                    // arm being measured.
                    let ceiling = if out.state == crate::syn::run::RunState::BudgetExhausted {
                        " ⚠ hit the ceiling"
                    } else {
                        ""
                    };
                    eprintln!(
                        "   {:<8} {}  {:>2} call(s)  {} round(s)  {:>5}ms  prompt {:>6} ch  {}{}",
                        if trial == 0 {
                            if stuffed { "stuffed" } else { "agentic" }
                        } else {
                            ""
                        },
                        if out.passed { "PASS" } else { "FAIL" },
                        out.tool_calls,
                        out.rounds,
                        out.elapsed_ms,
                        out.prompt_chars,
                        out.detail,
                        ceiling,
                    );
                    if !out.passed {
                        eprintln!(
                            "      → {}",
                            out.reply.replace('\n', " ").chars().take(200).collect::<String>()
                        );
                    }

                    totals[i].0 += usize::from(out.passed);
                    totals[i].1 += out.tool_calls;
                    totals[i].2 += out.elapsed_ms;
                    totals[i].3 += out.prompt_chars;
                    totals[i].4 +=
                        usize::from(out.state == crate::syn::run::RunState::BudgetExhausted);
                }
                if trials > 1 {
                    eprintln!("            └ {passes}/{trials}");
                }
            }
            eprintln!();
        }

        let n = QUESTIONS.len() * trials;
        eprintln!("═══ totals over {n} questions ════════════════════");
        for (i, name) in ["stuffed", "agentic"].into_iter().enumerate() {
            let (passed, calls, ms, prompt, cut_short) = totals[i];
            eprintln!(
                "{name:<8} {passed}/{n} correct   {calls} tool call(s)   {ms}ms total   \
                 {prompt} chars of system prompt   {cut_short} run(s) cut short"
            );
        }
        eprintln!();
    }
}
