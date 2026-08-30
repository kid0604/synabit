//! What Syn can do, and how.
//!
//! There used to be twenty tools, one per data model: `create_note`,
//! `create_task`, `create_event`, `search_vault`, `get_nodes_by_type`,
//! `person_brief`. That shape charged three times for every new kind of thing
//! — a Rust enum arm, a mini-app, and three or four tools — and the third
//! charge was the worst, because it was paid on *every* turn of *every*
//! conversation in tokens, and a longer list makes the model likelier to pick
//! the wrong entry from it. Worse, the assistant could only ever see the types
//! somebody had written tools for: a `book` the user invented was invisible.
//!
//! The tools are now shaped like the storage rather than like the apps. There
//! is one table of nodes, one query engine over it, and one write path that
//! takes any type — so there is one tool to search, one to read, one to
//! create, one to change, and `list_schemas` to say what is there. Those five
//! reach every type in the vault, including ones this app has never heard of.
//!
//! Six specialised tools survive, and each earns it by reaching a store the
//! node tools cannot: feed articles have their own table, file search runs
//! over extracted document text, and finance keeps its transactions inside a
//! month node as an array, which no node query can add up or append to.

use serde_json::Value;

use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::models::syn::{FunctionDefinition, ToolDefinition};
use tauri::Emitter;

/// Where a new node of a given type is written.
///
/// Mirrors `folderForType` in `src/shared/nodeRoutes.ts`, and a test asserts
/// the two agree — they are the only two writers of new nodes, and a vault
/// where the assistant files books somewhere the app does not is a vault with
/// two conventions.
///
/// Everything except tasks and events used to land in `Notes/`. Not wrong
/// about the data, since the `type:` in the frontmatter is what the scan
/// reads, but it puts cats among the notes when the vault is opened in a file
/// browser — and being readable without the app is most of the point.
pub(crate) fn folder_for_type(node_type: &str) -> String {
    match node_type {
        "task" => "Tasks".to_string(),
        "project" => "Projects".to_string(),
        "event" => "Events".to_string(),
        "person" => "People".to_string(),
        "note" => "Notes".to_string(),
        "quickcap" => "QuickCaps".to_string(),
        "whiteboard" => "Whiteboards".to_string(),
        other => {
            let clean = other.trim();
            if clean.is_empty() {
                return "Notes".to_string();
            }
            let mut chars = clean.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Notes".to_string(),
            }
        }
    }
}

/// Maximum characters allowed in a single tool result.
/// Results exceeding this are truncated with a marker.
const MAX_RESULT_CHARS: usize = 8000;
const MAX_CONTENT_CHARS: usize = 4000;

/// Context passed to tool execution, providing access to DB, vault path, and app handle.
/// Write tools need vault_path and app; read tools only need db.
///
/// Generic over the Tauri runtime for the same reason `scan_vault_into_db` is:
/// `tauri::AppHandle` names the real one, and nothing in a test can produce it.
/// Without this the write tools — which is to say everything Syn changes about
/// a vault — could only ever be exercised by hand.
pub struct ToolContext<'a, R: tauri::Runtime> {
    /// The database, unlocked.
    ///
    /// Held as the state rather than an open guard so that a tool which writes
    /// can call `write_node_inner`, which takes the lock itself. The caller
    /// used to lock once around every tool call and hand the guard down, which
    /// made the one shared write path unreachable from here — the mutex is not
    /// reentrant, so calling it would have deadlocked rather than failed.
    ///
    /// Each tool now locks for its own duration, which is shorter than before.
    pub db: &'a crate::db::DbState,
    pub vault_path: &'a str,
    pub app: &'a tauri::AppHandle<R>,
}

/// The database, for the length of one tool call.
fn lock<'a, R: tauri::Runtime>(
    ctx: &ToolContext<'a, R>,
) -> AppResult<std::sync::MutexGuard<'a, crate::db::DbBridge>> {
    ctx.db
        .lock()
        .map_err(|e| AppError::General(format!("DB lock error during tool call: {e}")))
}

// ═══════════════════════════════════════════════════════════════
//  TOOL DEFINITIONS
// ═══════════════════════════════════════════════════════════════

/// Build the complete list of tool definitions for the Ollama chat API.
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "query_nodes".to_string(),
                description: "Search and filter everything in the vault: notes, tasks, events, people, projects, and any type the user invented. This is the main tool — prefer it over guessing. Free words are full-text search; the filters below are combined with AND. Call list_schemas first if you do not know what types or fields this vault uses.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Query string. Filters: `type:task` restricts to a type; `#work` requires a tag; `status:reading` matches any frontmatter field; `-status:done` excludes a field value — use this for 'not finished', since a node that never had the field still counts as not having the value; `-draft` excludes a word; `rating:>3` and `due_date:<2026-09-01` compare; `sort:-updated_at` orders (prefix `-` for descending); `columns:title,author` chooses what comes back; `limit:20` caps the rows, while `total_matches` in the reply is the real count regardless — ask for `limit:1` when you only want the number. Free words outside a filter are searched in titles and bodies. Examples: `type:task -status:done` for open tasks, `type:book rating:>3`, `hợp đồng #work`."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_node".to_string(),
                description: "Read one node in full: its whole body plus every frontmatter field. query_nodes returns rows for scanning; use this when you need the actual contents of one thing you found.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id"],
                    "properties": {
                        "node_id": {
                            "type": "string",
                            "description": "The node's id, which is its path in the vault, e.g. 'Notes/Meeting.md'. Take it from a query_nodes result."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "list_schemas".to_string(),
                description: "Describe this vault: every type of thing in it, how many there are, and which frontmatter fields each type actually uses. Call this when you do not know what the user keeps, before searching for a type you are not sure exists, or before creating something of an unfamiliar type so you match the fields they already use.".to_string(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_node".to_string(),
                description: "Create anything in the vault — a note, a task, an event, or a type this app has never heard of. Check list_schemas first so the fields match what the user already uses for that type.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_type", "title"],
                    "properties": {
                        "node_type": {
                            "type": "string",
                            "description": "What kind of thing this is: 'note', 'task', 'event', 'person', 'project', or any type the user already uses. Lowercase."
                        },
                        "title": { "type": "string", "description": "The title." },
                        "content": {
                            "type": "string",
                            "description": "Markdown body. Optional."
                        },
                        "properties": {
                            "type": "object",
                            "description": "Frontmatter fields, as an object. For a task: status (todo/in_progress/done/backlog/canceled), due_date and start_date as YYYY-MM-DD, priority, tags. For an event: start_date, end_date. Any other field is allowed and is kept as written."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "update_node".to_string(),
                description: "Change fields on an existing node — mark a task done, set a due date, add a tag, edit any frontmatter field. Only the fields you send are touched; everything else on the node is left exactly as it was. A node's type can never be changed.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id", "properties"],
                    "properties": {
                        "node_id": {
                            "type": "string",
                            "description": "The node's id, from a query_nodes result."
                        },
                        "properties": {
                            "type": "object",
                            "description": "Only the fields to change, e.g. {\"status\": \"done\"}. Send null as a value to remove a field. Fields you do not mention keep their current values."
                        },
                        "content": {
                            "type": "string",
                            "description": "Replaces the whole body. Omit this to leave the body untouched — which is what a field-only change should do."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_linked_nodes".to_string(),
                description: "Follow the links out of and into a node — what it mentions, and what mentions it. Use this to explore around something you already found; query_nodes cannot express 'related to'.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id"],
                    "properties": {
                        "node_id": { "type": "string", "description": "The node's id." },
                        "direction": {
                            "type": "string",
                            "enum": ["outgoing", "incoming", "both"],
                            "description": "Which way to follow the links. Defaults to both."
                        }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_feed_articles".to_string(),
                description: "Search articles pulled in from the user's RSS feeds. These are not vault nodes and query_nodes cannot reach them.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string", "description": "Search query for feed articles" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_files".to_string(),
                description: "Search files in the vault's Files app by their contents, filename, extension, tags, or linked people. Use this when the user asks about files, images, documents, PDFs, or anything they believe is written inside a document. The 'query' parameter searches the text inside documents (PDF, Word, PowerPoint, spreadsheets, EPUB, HTML, plain text and code) as well as filenames and linked people names. Returns file metadata including path, size, extension, tags, people, and an 'excerpt' quoting the passage that matched when the match came from inside the document.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Text to look for inside documents, in filenames, or in linked people's names" },
                        "extension": { "type": "string", "description": "Filter by file extension, e.g. 'pdf'" },
                        "tag": { "type": "string", "description": "Filter by tag" },
                        "person": { "type": "string", "description": "Filter by a linked person's name" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_finance_summary".to_string(),
                description: "Totals and category breakdown for the user's money this month: income, expenses, balance, budgets. Finance transactions live inside a month node rather than as separate nodes, so query_nodes cannot add them up.".to_string(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_finance".to_string(),
                description: "Search financial records (transactions, budgets, accounts) in the vault. Splits the query into search terms and matches against finance nodes.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string", "description": "Search query for financial records" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_transaction".to_string(),
                description: "Record a financial transaction — money spent, earned, or moved between accounts. Transactions live inside the month's finance node rather than as nodes of their own, so create_node cannot make one. Call get_finance_summary first to learn which accounts and categories this user actually has.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["amount", "category"],
                    "properties": {
                        "amount": { "type": "number", "description": "Amount, as a positive number" },
                        "type": { "type": "string", "enum": ["income", "expense", "transfer"], "description": "Defaults to expense" },
                        "category": { "type": "string", "description": "Category name, matching one the user already uses" },
                        "account": { "type": "string", "description": "Account name. Defaults to the user's first account." },
                        "note": { "type": "string", "description": "What it was for" },
                        "date": { "type": "string", "description": "YYYY-MM-DD. Defaults to today." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_transactions".to_string(),
                description: "List financial transactions for a specific month. Shows type, amount, category, account, date, and note for each transaction.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "month": { "type": "string", "description": "Month in YYYY-MM format (e.g., '2026-06'). Defaults to current month." },
                        "type": { "type": "string", "enum": ["income", "expense", "transfer"], "description": "Optional filter by transaction type" },
                        "limit": { "type": "number", "description": "Maximum number of transactions to return. Defaults to 20." }
                    }
                }),
            },
        },
    ]
}

// ═══════════════════════════════════════════════════════════════
//  TOOL EXECUTOR DISPATCH
// ═══════════════════════════════════════════════════════════════

/// Execute a tool by name with the given arguments.
///
/// Returns a JSON string result that will be sent to Ollama as the content
/// of a `tool` role message. On failure, returns a JSON error object rather
/// than propagating the error, so the LLM can gracefully handle it.
pub fn execute_tool<R: tauri::Runtime>(
    ctx: &ToolContext<R>,
    name: &str,
    args: &Value,
) -> AppResult<String> {
    log::info!("[Syn Tools] Executing tool: {} with args: {}", name, args);

    let result = match name {
        // Generic — these reach every type in the vault, including ones this
        // app has never heard of.
        "query_nodes" => tool_query_nodes(&*lock(ctx)?, args),
        "get_node" => tool_get_node(&*lock(ctx)?, args),
        "list_schemas" => tool_list_schemas(&*lock(ctx)?),
        "create_node" => tool_create_node(ctx, args),
        "update_node" => tool_update_node(ctx, args),
        "get_linked_nodes" => tool_get_linked_nodes(&*lock(ctx)?, args),

        // Stores that are not nodes, or not node-shaped: feed articles have
        // their own table, file search filters on indexed document text, and
        // finance keeps its transactions inside a month node as an array,
        // which no node query can add up.
        "search_feed_articles" => tool_search_feed_articles(&*lock(ctx)?, args),
        "search_files" => tool_search_files(&*lock(ctx)?, args),
        "get_finance_summary" => tool_get_finance_summary(&*lock(ctx)?),
        "search_finance" => tool_search_finance(&*lock(ctx)?, args),
        "get_transactions" => tool_get_transactions(&*lock(ctx)?, args),
        "create_transaction" => tool_create_transaction(ctx, args),

        _ => return Err(AppError::General(format!("Unknown tool: {}", name))),
    };

    // Ensure the result is truncated to the size limit
    match result {
        Ok(json_str) => Ok(truncate_result(&json_str)),
        Err(e) => {
            log::error!("[Syn Tools] Tool '{}' failed: {}", name, e);
            Ok(serde_json::json!({"error": format!("{}", e)}).to_string())
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  TOOL IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════

/// Everything about one person, named the way somebody would name them.

/// A person's vault path, from a name or a path.
///
/// An exact name first, so two people whose names overlap — "An" and "An
/// Nguyễn" — do not answer for each other.

/// People filtered by how the relationship stands, not by name.

/// 1. search_vault — Universal FTS5 search

/// 2. get_node — Read full node content
fn tool_get_node(db: &DbBridge, args: &Value) -> AppResult<String> {
    let node_id = args
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_id".to_string()))?;

    let node = db.get_node(node_id)?;

    match node {
        Some(n) => {
            // Truncate content to 4000 chars to stay within tool result limits
            let content: String = n.content.chars().take(MAX_CONTENT_CHARS).collect();
            let content_truncated = content.len() < n.content.len();

            let output = serde_json::json!({
                "id": n.id,
                "type": n.node_type,
                "title": n.title,
                "content": content,
                "content_truncated": content_truncated,
                "properties": n.properties,
                "created_at": n.created_at,
                "updated_at": n.updated_at,
            });
            Ok(output.to_string())
        }
        None => Ok(serde_json::json!({"error": "Node not found", "node_id": node_id}).to_string()),
    }
}

/// 3. get_active_tasks_and_events — Upcoming deadlines

/// 4. get_nodes_by_type — List nodes by type (metadata only)

/// 5. search_feed_articles — Search RSS articles
fn tool_search_feed_articles(db: &DbBridge, args: &Value) -> AppResult<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: query".to_string()))?;

    let articles = db.search_feed_articles_for_rag(query, 10);

    let results: Vec<Value> = articles
        .iter()
        .map(|(id, title, summary, published_at)| {
            // Truncate summary to 300 chars
            let short_summary: String = summary.chars().take(300).collect();
            serde_json::json!({
                "id": id,
                "title": title,
                "summary": short_summary,
                "published_at": published_at,
            })
        })
        .collect();

    let output = serde_json::json!({
        "results": results,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

/// 6. get_nodes_by_tag — Filter by tag

/// 7. get_linked_nodes — Backlinks for a node
fn tool_get_linked_nodes(db: &DbBridge, args: &Value) -> AppResult<String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".to_string()))?;

    let node_id = args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");

    let nodes = db.get_linked_nodes(title, node_id)?;

    let results: Vec<Value> = nodes
        .iter()
        .take(20)
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "title": n.title,
                "type": n.node_type,
                "updated_at": n.updated_at,
            })
        })
        .collect();

    let total = nodes.len();
    let output = serde_json::json!({
        "results": results,
        "_total": total,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

/// 8. get_all_tags — Tag overview

/// 9. get_node_edges — Knowledge graph edges for a node

/// 10. search_finance — Financial records
fn tool_search_finance(db: &DbBridge, args: &Value) -> AppResult<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: query".to_string()))?;

    // Split query into individual terms for the LIKE-based search
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();

    let records = db.search_finance_nodes_for_rag(&terms, 15);

    let results: Vec<Value> = records
        .iter()
        .map(|(id, title, content, properties)| {
            // Truncate content for the result
            let short_content: String = content.chars().take(300).collect();
            // Parse properties JSON if possible
            let props: Value =
                serde_json::from_str(properties).unwrap_or(Value::String(properties.clone()));
            serde_json::json!({
                "id": id,
                "title": title,
                "content": short_content,
                "properties": props,
            })
        })
        .collect();

    let output = serde_json::json!({
        "results": results,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

/// 11. search_files — Search files by name, extension, tags, or linked people
fn tool_search_files(db: &DbBridge, args: &Value) -> AppResult<String> {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let extension = args.get("extension").and_then(|v| v.as_str()).unwrap_or("");
    let tag = args.get("tag").and_then(|v| v.as_str()).unwrap_or("");
    let person = args.get("person").and_then(|v| v.as_str()).unwrap_or("");

    // Use SQL-level filtering instead of loading all files into memory
    let nodes = db.search_files_filtered(query, extension, tag, person, 30)?;

    let results: Vec<Value> = nodes
        .iter()
        .map(|n| {
            let ext = n
                .properties
                .get("extension")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let size = n
                .properties
                .get("size")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let path = n
                .properties
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let tags = n
                .properties
                .get("tags")
                .cloned()
                .unwrap_or(serde_json::json!([]));
            let people = n
                .properties
                .get("people")
                .cloned()
                .unwrap_or(serde_json::json!([]));

            // Why this file matched, when the reason was something written
            // inside it rather than what it was called.
            let excerpt = db.file_text_excerpt(&n.id, query, 120);

            serde_json::json!({
                "id": n.id,
                "filename": n.title,
                "extension": ext,
                "size_bytes": size,
                "path": path,
                "tags": tags,
                "people": people,
                "updated_at": n.updated_at,
                "excerpt": excerpt,
            })
        })
        .collect();

    let output = serde_json::json!({
        "results": results,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

// ═══════════════════════════════════════════════════════════════
//  WRITE TOOL IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════

/// Create a node, through the one path the app itself uses.
///
/// This used to be a second implementation: write the file, upsert the row,
/// index the text, emit an event, done. What it skipped was everything the app
/// does underneath the frontmatter — registering the vault identity, assigning
/// the node's `node_id` and writing it into the file, recording that id against
/// the path, and handing the content to the CRDT bridge.
///
/// Nothing was lost by that, because sync notices a local change by hashing
/// files rather than by watching for CRDT operations. But a node the assistant
/// made and a node the app made were different objects until something else
/// came along and reconciled them, and none of the differences were written
/// down anywhere. One path means there is nothing to keep in step.
fn write_tool_node<R: tauri::Runtime>(
    ctx: &ToolContext<R>,
    node_type: &str,
    title: &str,
    content: &str,
    properties: serde_json::Value,
) -> AppResult<(String, String)> {
    use crate::commands::nodes::{free_node_path, write_node_inner};

    // Sanitize title for filename: remove unsafe characters
    let safe_title: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut safe_title = safe_title.trim().to_string();
    // A leading dot makes a dotfile, and the vault walk skips those — the note
    // would be written, indexed, and then vanish on the next scan. Same
    // underscore the other unsafe characters get.
    if safe_title.starts_with('.') {
        safe_title.replace_range(0..1, "_");
    }
    if safe_title.is_empty() {
        safe_title = "Untitled".to_string();
    }

    let vault = std::path::Path::new(ctx.vault_path);
    let rel_path = free_node_path(
        vault,
        &format!("{}/{}.md", folder_for_type(node_type), safe_title),
    );

    write_node_inner(
        ctx.app,
        ctx.db,
        ctx.vault_path.to_string(),
        rel_path.clone(),
        title.to_string(),
        node_type.to_string(),
        properties,
        Some(content.to_string()),
    )?;

    // `write_node_inner` emits nothing: the command it was split out of is
    // called from the frontend, which already knows what it just saved. A tool
    // call is the one write nobody on this side asked for, so the screens are
    // told here.
    let _ = ctx.app.emit(
        "node:created",
        serde_json::json!({
            "id": rel_path,
            "node_type": node_type,
            "title": title,
        }),
    );

    Ok((rel_path, title.to_string()))
}

// ═══════════════════════════════════════════════════════════════
//  GENERIC TOOLS
// ═══════════════════════════════════════════════════════════════

/// Search and filter every node, whatever its type.
///
/// This is one call onto the query engine the app already runs for query
/// blocks in notes, for the Tasks search bar and for saved filters. It
/// replaced five hard-coded tools — full-text search, by-type, by-tag, active
/// tasks and events, and people — none of which could see a type nobody had
/// written a tool for.
fn tool_query_nodes(db: &DbBridge, args: &Value) -> AppResult<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: query".into()))?;

    let parsed = crate::search::parse_query(query);
    let result = db.run_node_query(&parsed)?;

    let rows: Vec<Value> = result
        .rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "type": r.node_type,
                "title": r.title,
                "columns": r.cells,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "columns": result.columns,
        "results": rows,
        // `total` is what matched, `_returned` is what fitted under the limit.
        // Reporting only the second would let the model answer "you have 20
        // overdue tasks" when the true number is several hundred.
        "total_matches": result.total,
        "_returned": rows.len(),
    })
    .to_string())
}

/// What this vault contains, in the vault's own vocabulary.
///
/// The convergence point of the whole malleability argument, in its cheapest
/// possible form: there is no schema anywhere to read, so this reports what is
/// observably there. It is what lets the assistant work with a type nobody
/// wrote code for — it can see that `book` exists and that books here carry
/// `author`, `rating` and `status`, and then query and create them.
fn tool_list_schemas(db: &DbBridge) -> AppResult<String> {
    // Enough keys to describe a type, few enough that one node with a large
    // generated blob cannot crowd out the other types.
    const KEYS_PER_TYPE: usize = 25;

    let schemas: Vec<Value> = db
        .observed_schemas(KEYS_PER_TYPE)?
        .into_iter()
        .map(|(node_type, count, fields)| {
            serde_json::json!({
                "type": node_type,
                "count": count,
                "fields": fields,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "types": schemas,
        "_note": "Fields are what nodes of this type actually carry, not a list of what is allowed. A node may be missing any of them, and may carry others.",
    })
    .to_string())
}

/// Create a node of any type.
///
/// Replaced `create_note`, `create_task`, `create_event` and
/// `create_transaction`, which between them could make four things. This can
/// make anything, because `write_node_file` has always accepted an arbitrary
/// type string and the frontmatter writer has always kept what it was given.
fn tool_create_node<R: tauri::Runtime>(
    ctx: &ToolContext<R>, args: &Value) -> AppResult<String> {
    let node_type = args
        .get("node_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_type".into()))?
        .trim()
        .to_lowercase();

    if node_type.is_empty() {
        return Err(AppError::General("node_type cannot be empty".into()));
    }

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".into()))?;
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

    let mut properties = match args.get("properties") {
        Some(Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };

    // A task with no status is invisible to every bucket and filter in the
    // Tasks app. `create_task` used to set this; nothing else would.
    if node_type == "task" && !properties.contains_key("status") {
        properties.insert("status".to_string(), serde_json::json!("todo"));
    }

    let (id, created_title) = write_tool_node(
        ctx,
        &node_type,
        title,
        content,
        Value::Object(properties),
    )?;

    Ok(serde_json::json!({
        "success": true,
        "id": id,
        "type": node_type,
        "title": created_title,
        "message": format!("Created {} '{}'", node_type, created_title),
    })
    .to_string())
}

/// Change fields on a node, leaving everything it did not mention alone.
///
/// The patch semantics are not a convenience — they are what makes a generic
/// writer safe. A tool that rebuilt frontmatter from its arguments would erase
/// every field the model did not happen to know about, which on a
/// user-invented type is all of them.
///
/// The type is never written from an argument. `nodeRoutes.ts` records what
/// happens when a writer decides a node's type for itself: a task opened in
/// the note editor was saved as a note on the first autosave and the task was
/// gone. Here the type comes from the node on disk and nowhere else.
fn tool_update_node<R: tauri::Runtime>(
    ctx: &ToolContext<R>, args: &Value) -> AppResult<String> {
    use crate::commands::nodes::{
        existing_body, existing_properties, markdown_with_frontmatter, resolve_properties,
    };

    let node_id = args
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_id".into()))?;

    let patch = args
        .get("properties")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let Some(mut node) = lock(ctx)?.get_node(node_id)? else {
        return Ok(
            serde_json::json!({ "error": "Node not found", "node_id": node_id }).to_string(),
        );
    };

    let full_path = std::path::Path::new(ctx.vault_path).join(&node.id);
    let ext = full_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md")
        .to_string();

    let mut properties = resolve_properties(existing_properties(&full_path, &ext), &patch);

    // `completed_at` is derived from `status` by every other writer in the
    // app, and the Tasks views read it. A generic write that set one without
    // the other would leave a task that looks done and is not dated, which is
    // worse than either state on its own.
    if node.node_type == "task" {
        if let Some(status) = patch.get("status").and_then(|v| v.as_str()) {
            if let Some(obj) = properties.as_object_mut() {
                let stamp = if status == "done" {
                    serde_json::json!(chrono::Utc::now().format("%Y-%m-%d").to_string())
                } else {
                    serde_json::json!("")
                };
                obj.insert("completed_at".to_string(), stamp);
            }
        }
    }

    // A body only changes when one was sent. Omitting it is how a field-only
    // update says "leave what I wrote alone".
    let body = match args.get("content").and_then(|v| v.as_str()) {
        Some(new_body) => new_body.to_string(),
        None => existing_body(&full_path, &ext),
    };

    let title = properties
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(&node.title)
        .to_string();

    if ext == "md" {
        let file_content =
            markdown_with_frontmatter(&title, &node.node_type, &properties, &body);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_path, &file_content)?;
    } else {
        return Ok(serde_json::json!({
            "error": format!("Cannot edit a .{} node; only markdown nodes can be updated here", ext),
            "node_id": node_id,
        })
        .to_string());
    }

    let now = chrono::Utc::now();
    node.title = title.clone();
    node.content = body.clone();
    node.properties = properties.clone();
    node.updated_at = now.to_rfc3339();
    node.timestamp = now.timestamp_millis();
    lock(ctx)?.upsert_node(&node)?;

    let tags_str = properties
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let props_json = serde_json::to_string(&properties).unwrap_or_default();
    lock(ctx)?.upsert_search_entry(
        &node.id,
        &node.node_type,
        &title,
        &tags_str,
        &body,
        &props_json,
        properties.get("status").and_then(|v| v.as_str()),
        &node.updated_at,
        &node.id,
    );

    let _ = ctx.app.emit(
        "node:updated",
        serde_json::json!({
            "id": node.id,
            "node_type": node.node_type,
            "title": title,
        }),
    );

    let changed: Vec<&String> = patch
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();

    Ok(serde_json::json!({
        "success": true,
        "id": node.id,
        "type": node.node_type,
        "title": title,
        "changed": changed,
    })
    .to_string())
}

/// 12. create_note

/// 13. create_task

/// 14. update_task_status

/// 15. create_event

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

/// Truncate a JSON result string to `MAX_RESULT_CHARS`.
/// If truncated, appends a marker so the LLM knows the data was cut off.
fn truncate_result(s: &str) -> String {
    if s.chars().count() <= MAX_RESULT_CHARS {
        return s.to_string();
    }

    let truncated: String = s.chars().take(MAX_RESULT_CHARS).collect();
    format!("{}... (truncated)", truncated)
}

// ═══════════════════════════════════════════════════════════════
//  FINANCE TOOL IMPLEMENTATIONS
// ═══════════════════════════════════════════════════════════════

/// 16. get_finance_summary — Overview of user's financial state
fn tool_get_finance_summary(db: &DbBridge) -> AppResult<String> {
    // Read the Finance Config node
    let config_node = db.get_node("Finance/Config.json")?;

    let (accounts, income_categories, expense_categories, currency) = match &config_node {
        Some(node) => {
            let meta = &node.properties;
            let accounts = meta
                .get("accounts")
                .cloned()
                .unwrap_or(serde_json::json!([]));
            let income_cats = meta
                .get("incomeCategories")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let expense_cats = meta
                .get("expenseCategories")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let currency = meta
                .get("currency")
                .and_then(|v| v.as_str())
                .unwrap_or("VND")
                .to_string();
            (accounts, income_cats, expense_cats, currency)
        }
        None => {
            return Ok(serde_json::json!({
                "error": "Finance not set up. The user has not configured Finance yet.",
                "hint": "Ask the user to open the Finance app and set up their accounts first."
            })
            .to_string());
        }
    };

    // Read current month's transactions for summary
    let now = chrono::Local::now();
    let month_key = now.format("%Y-%m").to_string();
    let month_node_id = format!("Finance/{}.json", month_key);
    let month_node = db.get_node(&month_node_id)?;

    let (total_income, total_expense, tx_count) = match &month_node {
        Some(node) => {
            let txs = node
                .properties
                .get("transactions")
                .and_then(|v| v.as_array());
            match txs {
                Some(arr) => {
                    let mut income = 0.0_f64;
                    let mut expense = 0.0_f64;
                    for tx in arr {
                        let amount = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        match tx.get("type").and_then(|v| v.as_str()) {
                            Some("income") => income += amount,
                            Some("expense") => expense += amount,
                            _ => {}
                        }
                    }
                    (income, expense, arr.len())
                }
                None => (0.0, 0.0, 0),
            }
        }
        None => (0.0, 0.0, 0),
    };

    // Calculate current balances per account
    // Balance = initialBalance + all income to account - all expense from account + transfers in - transfers out
    let account_balances = compute_account_balances(db, &accounts);

    let output = serde_json::json!({
        "currency": currency,
        "accounts": account_balances,
        "income_categories": income_categories,
        "expense_categories": expense_categories,
        "this_month": {
            "month": month_key,
            "total_income": total_income,
            "total_expense": total_expense,
            "net": total_income - total_expense,
            "transaction_count": tx_count
        }
    });

    Ok(output.to_string())
}

/// Helper: compute current balance for each account across all months
fn compute_account_balances(db: &DbBridge, accounts_val: &Value) -> Value {
    let accounts_arr = match accounts_val.as_array() {
        Some(a) => a,
        None => return serde_json::json!([]),
    };

    // Get all finance_month nodes
    let month_nodes = db.get_nodes_by_type("finance_month").unwrap_or_default();

    // Build a map of account_id -> running balance delta
    let mut deltas: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for node in &month_nodes {
        if let Some(txs) = node
            .properties
            .get("transactions")
            .and_then(|v| v.as_array())
        {
            for tx in txs {
                let amount = tx.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let acc_id = tx.get("accountId").and_then(|v| v.as_str()).unwrap_or("");
                let tx_type = tx.get("type").and_then(|v| v.as_str()).unwrap_or("");

                match tx_type {
                    "income" => {
                        *deltas.entry(acc_id.to_string()).or_insert(0.0) += amount;
                    }
                    "expense" => {
                        *deltas.entry(acc_id.to_string()).or_insert(0.0) -= amount;
                    }
                    "transfer" => {
                        *deltas.entry(acc_id.to_string()).or_insert(0.0) -= amount;
                        if let Some(to_acc) = tx.get("toAccountId").and_then(|v| v.as_str()) {
                            *deltas.entry(to_acc.to_string()).or_insert(0.0) += amount;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Build result with initial + delta
    let results: Vec<Value> = accounts_arr
        .iter()
        .map(|acc| {
            let id = acc.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let name = acc.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let initial = acc
                .get("initialBalance")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let delta = deltas.get(id).copied().unwrap_or(0.0);
            serde_json::json!({
                "id": id,
                "name": name,
                "balance": initial + delta
            })
        })
        .collect();

    serde_json::json!(results)
}

/// 17. create_transaction — Create a financial transaction

/// 18. get_transactions — List transactions for a specific month
fn tool_get_transactions(db: &DbBridge, args: &Value) -> AppResult<String> {
    let now = chrono::Local::now();
    let month = args
        .get("month")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| now.format("%Y-%m").to_string());
    let type_filter = args.get("type").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let month_node_id = format!("Finance/{}.json", month);
    let month_node = db.get_node(&month_node_id)?;

    let transactions = match &month_node {
        Some(node) => node
            .properties
            .get("transactions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        None => Vec::new(),
    };

    // Filter by type if specified
    let filtered: Vec<&Value> = transactions
        .iter()
        .filter(|tx| {
            if let Some(filter) = type_filter {
                tx.get("type").and_then(|v| v.as_str()) == Some(filter)
            } else {
                true
            }
        })
        .collect();

    // Sort by date descending (most recent first)
    let mut sorted: Vec<&Value> = filtered;
    sorted.sort_by(|a, b| {
        let da = a.get("date").and_then(|v| v.as_str()).unwrap_or("");
        let db_date = b.get("date").and_then(|v| v.as_str()).unwrap_or("");
        db_date.cmp(da)
    });

    // Apply limit
    let limited: Vec<Value> = sorted
        .into_iter()
        .take(limit)
        .map(|v| {
            // Slim down for LLM — only essential fields
            serde_json::json!({
                "id": v.get("id"),
                "type": v.get("type"),
                "amount": v.get("amount"),
                "category": v.get("category"),
                "accountId": v.get("accountId"),
                "date": v.get("date"),
                "note": v.get("note")
            })
        })
        .collect();

    // Read config for currency
    let config_node = db.get_node("Finance/Config.json")?;
    let currency = config_node
        .as_ref()
        .and_then(|n| n.properties.get("currency"))
        .and_then(|v| v.as_str())
        .unwrap_or("VND");

    // Calculate totals
    let total_income: f64 = transactions
        .iter()
        .filter(|tx| tx.get("type").and_then(|v| v.as_str()) == Some("income"))
        .filter_map(|tx| tx.get("amount").and_then(|v| v.as_f64()))
        .sum();
    let total_expense: f64 = transactions
        .iter()
        .filter(|tx| tx.get("type").and_then(|v| v.as_str()) == Some("expense"))
        .filter_map(|tx| tx.get("amount").and_then(|v| v.as_f64()))
        .sum();

    let output = serde_json::json!({
        "month": month,
        "currency": currency,
        "total_income": total_income,
        "total_expense": total_expense,
        "net": total_income - total_expense,
        "total_transactions": transactions.len(),
        "results": limited,
        "_returned": limited.len()
    });

    Ok(output.to_string())
}

/// Helper: Write a JSON node file to disk + upsert DB + emit event.
/// This matches the write_node_file format for .json files.

/// Helper: Simple random u16 for ID generation (matches frontend pattern)

/// Helper: Format amount with currency


// ═══════════════════════════════════════════════════════════════
//  TESTS
// ═══════════════════════════════════════════════════════════════

fn tool_create_transaction<R: tauri::Runtime>(
    ctx: &ToolContext<R>, args: &Value) -> AppResult<String> {
    let amount = args
        .get("amount")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AppError::General("Missing required parameter: amount".into()))?;
    let category = args
        .get("category")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: category".into()))?;

    if amount <= 0.0 {
        return Ok(serde_json::json!({"error": "Amount must be a positive number"}).to_string());
    }

    let tx_type = args
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("expense");
    if tx_type != "income" && tx_type != "expense" {
        return Ok(serde_json::json!({"error": format!("Invalid type '{}'. Must be 'income' or 'expense'.", tx_type)}).to_string());
    }

    let note = args.get("note").and_then(|v| v.as_str()).unwrap_or("");
    let now = chrono::Local::now();
    let date_str = args
        .get("date")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| now.format("%Y-%m-%d").to_string());

    // Read config to validate account and get defaults
    let config_node = lock(ctx)?.get_node("Finance/Config.json")?;
    let config_meta = match &config_node {
        Some(node) => &node.properties,
        None => {
            return Ok(serde_json::json!({
                "error": "Finance not set up. Ask user to open Finance app first."
            })
            .to_string());
        }
    };

    // Determine account_id
    let account_id = args
        .get("account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Default to first account
            config_meta
                .get("accounts")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|acc| acc.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("acc-1")
                .to_string()
        });

    // Get account name for confirmation message
    let account_name = config_meta
        .get("accounts")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(&account_id))
        })
        .and_then(|a| a.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    // Generate transaction ID
    let tx_id = format!(
        "tx-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        rand_u16()
    );

    // Build the transaction object (matches frontend Transaction interface exactly)
    let transaction = serde_json::json!({
        "id": tx_id,
        "type": tx_type,
        "amount": amount,
        "category": category,
        "accountId": account_id,
        "date": format!("{}T00:00:00", date_str),
        "note": note
    });

    // Determine month key from date
    let month_key = if date_str.len() >= 7 {
        &date_str[..7]
    } else {
        &date_str
    };
    let month_node_id = format!("Finance/{}.json", month_key);

    // Read or create the month node
    let existing_month = lock(ctx)?.get_node(&month_node_id)?;
    let mut transactions: Vec<Value> = match &existing_month {
        Some(node) => node
            .properties
            .get("transactions")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        None => Vec::new(),
    };

    // Add the new transaction
    transactions.push(transaction);

    // Build the month properties
    let month_props = serde_json::json!({
        "transactions": transactions
    });

    // Construct the month title
    let month_parts: Vec<&str> = month_key.split('-').collect();
    let month_title = if month_parts.len() == 2 {
        format!("Month {}/{}", month_parts[1], month_parts[0])
    } else {
        format!("Month {}", month_key)
    };

    // Write JSON file to disk (matches write_node_file JSON format)
    write_json_node(
        ctx,
        &month_node_id,
        "finance_month",
        &month_title,
        &month_props,
    )?;

    // Get currency for display
    let currency = config_meta
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("VND");

    let output = serde_json::json!({
        "success": true,
        "id": tx_id,
        "type": tx_type,
        "amount": amount,
        "category": category,
        "account": account_name,
        "date": date_str,
        "note": note,
        "currency": currency,
        "message": format!("{} {} {} — {} ({})",
            if tx_type == "expense" { "💸" } else { "💰" },
            format_amount(amount, currency),
            category, note, account_name
        )
    });

    Ok(output.to_string())
}

fn write_json_node<R: tauri::Runtime>(
    ctx: &ToolContext<R>,
    rel_path: &str,
    node_type: &str,
    title: &str,
    properties: &Value,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    // Build properties with timestamps
    let mut props = properties.clone();
    if let Some(map) = props.as_object_mut() {
        if !map.contains_key("created_at") {
            // Check if node already exists to preserve created_at
            if let Ok(Some(existing)) = lock(ctx)?.get_node(rel_path) {
                let existing_created = existing
                    .properties
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&now);
                map.insert(
                    "created_at".to_string(),
                    Value::String(existing_created.to_string()),
                );
            } else {
                map.insert("created_at".to_string(), Value::String(now.clone()));
            }
        }
        map.insert("updated_at".to_string(), Value::String(now.clone()));
    }

    // Build JSON file content (matches nodes.rs write_node_file for .json)
    let json_obj = serde_json::json!({
        "title": title,
        "type": node_type,
        "metadata": props,
        "content": ""
    });
    let file_content = serde_json::to_string_pretty(&json_obj).unwrap_or_default();

    // Write to disk
    let full_path = std::path::Path::new(ctx.vault_path).join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full_path, &file_content)?;

    // Upsert into DB
    let timestamp = chrono::Utc::now().timestamp_millis();
    let created_at = props
        .get("created_at")
        .and_then(|v| v.as_str())
        .unwrap_or(&now)
        .to_string();

    let node = crate::models::node::NodeMetadata {
        id: rel_path.to_string(),
        node_type: node_type.to_string(),
        title: title.to_string(),
        content: String::new(),
        properties: props.clone(),
        created_at,
        updated_at: now.clone(),
        timestamp,
        blocks: None,
    };
    lock(ctx)?.upsert_node(&node)?;

    // Update search index
    let props_str = serde_json::to_string(&props).unwrap_or_default();
    lock(ctx)?.upsert_search_entry(
        rel_path, node_type, title, "", "", &props_str, None, &now, rel_path,
    );

    // Emit event for UI sync
    let _ = ctx.app.emit(
        "node:changed",
        serde_json::json!({
            "id": rel_path,
            "node_type": node_type,
            "title": title,
        }),
    );

    Ok(())
}

fn rand_u16() -> u16 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as u16
}

fn format_amount(amount: f64, currency: &str) -> String {
    if currency == "VND" {
        // VND: no decimals, use comma separator
        let int_amount = amount as i64;
        let formatted = format_number_with_separator(int_amount);
        format!("{}đ", formatted)
    } else {
        format!("{:.2} {}", amount, currency)
    }
}

fn format_number_with_separator(n: i64) -> String {
    let s = n.to_string();
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*c);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool offered to the model is one it can understand.
    ///
    /// This used to assert a count, which said nothing about whether the
    /// tools were usable and had to be edited every time one was added. A
    /// definition with an empty description is a tool the model never picks;
    /// one with a malformed schema is a tool it calls wrongly.
    #[test]
    fn every_tool_is_described_well_enough_to_be_picked() {
        for definition in get_tool_definitions() {
            let name = &definition.function.name;
            assert!(!name.trim().is_empty(), "a tool has no name");
            assert!(
                definition.function.description.len() > 20,
                "'{name}' is not described well enough for the model to know when to use it"
            );
            let schema = &definition.function.parameters;
            assert_eq!(
                schema.get("type").and_then(|t| t.as_str()),
                Some("object"),
                "'{name}' does not take an object"
            );
            assert!(
                schema.get("properties").is_some_and(|p| p.is_object()),
                "'{name}' declares no parameters, not even none"
            );
            for required in schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                let key = required.as_str().unwrap_or_default();
                assert!(
                    schema["properties"].get(key).is_some(),
                    "'{name}' requires '{key}' but never says what it is"
                );
            }
        }
    }

    #[test]
    fn no_two_tools_share_a_name() {
        // The model picks a tool by name; two with one name is a coin toss.
        let defs = get_tool_definitions();
        let mut names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "a tool name is used twice");
    }

    /// The assistant and the app file a new node in the same place.
    ///
    /// Two writers create nodes — `write_tool_node` here, and Things through
    /// `writeNode` — and they are in different languages with no link between
    /// them. A vault where the assistant puts books in `Notes/` and the app
    /// puts them in `Books/` has two conventions and no way to tell which is
    /// right, so this reads the frontend's rule and checks it against this one.
    #[test]
    fn the_frontend_files_a_new_node_where_the_assistant_does() {
        let source = include_str!("../../../src/shared/nodeRoutes.ts");
        let block = source
            .split("const TYPE_FOR_DIRECTORY: Readonly<Record<string, string>> = {")
            .nth(1)
            .expect("the directory map is declared")
            .split("};")
            .next()
            .expect("the declaration closes");

        let mut checked = 0;
        for line in block.lines() {
            let Some((folder, node_type)) = line.trim().trim_end_matches(',').split_once(':') else {
                continue;
            };
            let folder = folder.trim();
            let node_type = node_type.trim().trim_matches('\'');
            if folder.is_empty() || node_type.is_empty() {
                continue;
            }
            assert_eq!(
                folder_for_type(node_type),
                folder,
                "`{node_type}` goes to a different folder depending on who writes it"
            );
            checked += 1;
        }
        assert!(checked >= 7, "only read {checked} entries out of nodeRoutes.ts");

        // And the rule for everything else, which is where they would drift
        // apart most quietly, since neither side has a list to compare.
        assert_eq!(folder_for_type("animal"), "Animal");
        assert_eq!(folder_for_type("book"), "Book");
        assert_eq!(folder_for_type("cá"), "Cá");
        assert_eq!(folder_for_type(""), "Notes");
    }

    /// The set the model is offered, named one by one.
    ///
    /// Spelled out rather than counted, because the point of this list is not
    /// how many there are but *which*: the first five reach every type in the
    /// vault, and every other entry has to justify itself by reaching a store
    /// they cannot. A tool added here without that justification is the old
    /// per-model shape growing back.
    #[test]
    fn the_generic_tools_reach_every_type_and_the_rest_earn_their_place() {
        let defs = get_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();

        // Generic: these work on notes, tasks, people, and on `book` — a type
        // nobody wrote a line of code for.
        for generic in [
            "query_nodes",
            "get_node",
            "list_schemas",
            "create_node",
            "update_node",
            "get_linked_nodes",
        ] {
            assert!(names.contains(&generic), "the generic tool {generic} is missing");
        }

        // Specialised, and each for a reason: feed articles have their own
        // table, file search runs over extracted document text, and finance
        // keeps transactions inside a month node as an array.
        for specialised in [
            "search_feed_articles",
            "search_files",
            "get_finance_summary",
            "search_finance",
            "get_transactions",
            "create_transaction",
        ] {
            assert!(names.contains(&specialised), "{specialised} is missing");
        }

        assert_eq!(
            names.len(),
            12,
            "the tool list changed; every entry costs tokens on every turn of \
             every conversation, so a new one needs a store the generic tools \
             cannot reach: {names:?}"
        );
    }

    /// The per-model tools are gone and must not come back.
    ///
    /// Each of these could only ever see one kind of thing. Re-adding one is
    /// how the list grows back to twenty.
    #[test]
    fn no_tool_is_tied_to_a_single_data_model() {
        let defs = get_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();

        for retired in [
            "search_vault",
            "get_nodes_by_type",
            "get_nodes_by_tag",
            "get_active_tasks_and_events",
            "person_brief",
            "find_people",
            "get_all_tags",
            "get_node_edges",
            "create_note",
            "create_task",
            "create_event",
            "update_task_status",
        ] {
            assert!(
                !names.contains(&retired),
                "{retired} is back; `query_nodes`, `create_node` or `update_node` covers it"
            );
        }
    }

    /// The prompt and the tool list have to agree.
    ///
    /// They are written in different files and nothing links them, so a tool
    /// renamed on one side leaves the other telling the model to call
    /// something that does not exist — which reads to the user as the
    /// assistant refusing to do its job.
    #[test]
    fn the_system_prompt_only_names_tools_that_exist() {
        let prompt = crate::syn::rag::build_system_prompt("", "auto");
        let names: Vec<String> = get_tool_definitions()
            .iter()
            .map(|d| d.function.name.clone())
            .collect();

        for retired in [
            "search_vault",
            "get_nodes_by_type",
            "create_note",
            "create_task",
            "create_event",
            "update_task_status",
            "person_brief",
            "find_people",
            "get_all_tags",
            "get_node_edges",
        ] {
            assert!(
                !prompt.contains(retired),
                "the system prompt still tells the model to call `{retired}`, which no longer exists"
            );
        }

        // And the ones it does name are real.
        for named in ["query_nodes", "list_schemas", "create_node", "update_node"] {
            assert!(prompt.contains(named), "the prompt never mentions `{named}`");
            assert!(names.iter().any(|n| n == named));
        }
    }

    #[test]
    fn test_tool_definitions_are_functions() {
        let defs = get_tool_definitions();
        for def in &defs {
            assert_eq!(def.tool_type, "function");
        }
    }

    #[test]
    fn test_tool_definitions_have_descriptions() {
        let defs = get_tool_definitions();
        for def in &defs {
            assert!(
                !def.function.description.is_empty(),
                "Tool '{}' has empty description",
                def.function.name
            );
        }
    }

    #[test]
    fn test_tool_definitions_have_parameters() {
        let defs = get_tool_definitions();
        for def in &defs {
            assert!(
                def.function.parameters.is_object(),
                "Tool '{}' parameters should be an object",
                def.function.name
            );
            let params = def.function.parameters.as_object().expect("is object");
            assert_eq!(
                params.get("type").and_then(|v| v.as_str()),
                Some("object"),
                "Tool '{}' parameters.type should be 'object'",
                def.function.name
            );
        }
    }

    #[test]
    fn test_truncate_result_short() {
        let short = "hello world";
        assert_eq!(truncate_result(short), short);
    }

    #[test]
    fn test_truncate_result_long() {
        let long = "x".repeat(MAX_RESULT_CHARS + 1000);
        let result = truncate_result(&long);
        assert!(result.chars().count() < MAX_RESULT_CHARS + 1000);
        assert!(result.ends_with("... (truncated)"));
    }

    #[test]
    fn test_truncate_result_exact_limit() {
        let exact = "x".repeat(MAX_RESULT_CHARS);
        assert_eq!(truncate_result(&exact), exact);
    }
}
