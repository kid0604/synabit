//! Tool definitions and executors for Syn function calling (Phase 3).
//!
//! Defines 10 read-only tools that the LLM can call to query the user's vault
//! via the existing `DbBridge` methods. Each tool returns a JSON string that is
//! sent back to Ollama as a `tool` role message.

use serde_json::Value;

use crate::db::DbBridge;
use crate::error::{AppError, AppResult};
use crate::models::syn::{FunctionDefinition, ToolDefinition};
use tauri::Emitter;

/// Maximum characters allowed in a single tool result.
/// Results exceeding this are truncated with a marker.
const MAX_RESULT_CHARS: usize = 8000;
const MAX_CONTENT_CHARS: usize = 4000;

/// Context passed to tool execution, providing access to DB, vault path, and app handle.
/// Write tools need vault_path and app; read tools only need db.
pub struct ToolContext<'a> {
    pub db: &'a crate::db::DbBridge,
    pub vault_path: &'a str,
    pub app: &'a tauri::AppHandle,
}

// ═══════════════════════════════════════════════════════════════
//  TOOL DEFINITIONS
// ═══════════════════════════════════════════════════════════════

/// Build the complete list of tool definitions for the Ollama chat API.
pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        // 1. search_vault — Universal FTS5 search
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_vault".to_string(),
                description: "Search the user's vault using full-text search. Supports advanced syntax: is:task, is:note, #tag, status:done, -exclude, \"exact phrase\". Returns matching nodes with snippets.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "FTS5 search query. Supports: is:task, is:note, is:event, #tag, status:done, status:todo, -exclude, \"exact phrase\""
                        }
                    }
                }),
            },
        },
        // 2. get_node — Read full node content
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_node".to_string(),
                description: "Read the full content of a specific node (note, task, event, etc.) by its ID. Returns complete content, properties, and metadata.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id"],
                    "properties": {
                        "node_id": {
                            "type": "string",
                            "description": "The unique ID of the node to retrieve"
                        }
                    }
                }),
            },
        },
        // 3. get_active_tasks_and_events — Upcoming deadlines
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_active_tasks_and_events".to_string(),
                description: "Get all active tasks with due dates and upcoming events. Tasks marked as 'done' or 'canceled' are excluded. Returns task status, due dates, and priorities.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        },
        // 4. get_nodes_by_type — List nodes by type
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_nodes_by_type".to_string(),
                description: "List all nodes of a specific type. Returns metadata only (no full content). Sorted by last updated.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_type"],
                    "properties": {
                        "node_type": {
                            "type": "string",
                            "description": "Node type to filter by. One of: note, task, event, person, quickcap, file",
                            "enum": ["note", "task", "event", "person", "quickcap", "file"]
                        }
                    }
                }),
            },
        },
        // 5. search_feed_articles — Search RSS articles
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_feed_articles".to_string(),
                description: "Search saved RSS feed articles by keyword. Returns matching article titles, summaries, and publication dates.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for feed articles"
                        }
                    }
                }),
            },
        },
        // 6. get_nodes_by_tag — Filter by tag
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_nodes_by_tag".to_string(),
                description: "Get all nodes tagged with a specific tag. Returns node metadata without full content.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["tag"],
                    "properties": {
                        "tag": {
                            "type": "string",
                            "description": "Tag name to filter by (without the # prefix)"
                        }
                    }
                }),
            },
        },
        // 7. get_linked_nodes — Backlinks for a node
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_linked_nodes".to_string(),
                description: "Get all nodes that link to (reference) a given node. Useful for discovering backlinks and related content.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["title"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Title of the node to find backlinks for"
                        },
                        "node_id": {
                            "type": "string",
                            "description": "Optional node ID for more precise matching"
                        }
                    }
                }),
            },
        },
        // 8. get_all_tags — Tag overview
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_all_tags".to_string(),
                description: "Get an overview of all tags used in the vault with their usage counts. Useful for understanding how the user organizes their knowledge.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        },
        // 9. get_node_edges — Knowledge graph edges
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_node_edges".to_string(),
                description: "Get all knowledge graph edges (links, references, embeds) connected to a specific node. Shows how nodes relate to each other.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id"],
                    "properties": {
                        "node_id": {
                            "type": "string",
                            "description": "The node ID to get edges for"
                        }
                    }
                }),
            },
        },
        // 10. search_finance — Financial records
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_finance".to_string(),
                description: "Search financial records (transactions, budgets, accounts) in the vault. Splits the query into search terms and matches against finance nodes.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query for financial records"
                        }
                    }
                }),
            },
        },
        // 11. search_files — Search files by name, extension, tags, or linked people
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_files".to_string(),
                description: "Search files in the vault's Files app by filename, extension, tags, or linked people. Use this when the user asks about files, images, documents, PDFs, or any file-related queries. The 'query' parameter searches both filenames AND linked people names. Returns file metadata including path, size, extension, tags, and people.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search term to match against filenames AND linked people names (case-insensitive). Use this for person names like 'Lê Anh Khôi'."
                        },
                        "extension": {
                            "type": "string",
                            "description": "Filter by file extension (e.g. 'png', 'jpg', 'pdf', 'md'). Without the dot."
                        },
                        "tag": {
                            "type": "string",
                            "description": "Filter by tag assigned to the file"
                        },
                        "person": {
                            "type": "string",
                            "description": "Filter by person name linked to the file (case-insensitive substring match)"
                        }
                    }
                }),
            },
        },
        // 12. create_note — Create a new note
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_note".to_string(),
                description: "Create a new note in the user's vault. Use when the user asks to write, save, or create a note. Returns the created note's ID and path.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["title", "content"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Title of the note (will become the filename)"
                        },
                        "content": {
                            "type": "string",
                            "description": "Markdown content of the note"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional tags to assign"
                        }
                    }
                }),
            },
        },
        // 13. create_task — Create a new task
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_task".to_string(),
                description: "Create a new task in the user's vault. Use when the user wants to add a to-do, action item, or reminder.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["title"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Title of the task"
                        },
                        "content": {
                            "type": "string",
                            "description": "Optional detailed description"
                        },
                        "start_date": {
                            "type": "string",
                            "description": "Start date in YYYY-MM-DD format. When the task should begin."
                        },
                        "due_date": {
                            "type": "string",
                            "description": "Due date in YYYY-MM-DD format. When the task should be completed."
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["P1", "P2", "P3", "P4"],
                            "description": "Priority level. Only set if the user explicitly mentions priority."
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional tags"
                        }
                    }
                }),
            },
        },
        // 14. update_task_status — Update task status
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "update_task_status".to_string(),
                description: "Update the status of an existing task. Use when the user marks a task as done, in progress, or wants to change its priority/due date.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["node_id", "status"],
                    "properties": {
                        "node_id": {
                            "type": "string",
                            "description": "The ID of the task node to update"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["todo", "in_progress", "done", "canceled", "backlog"],
                            "description": "New status"
                        },
                        "due_date": {
                            "type": "string",
                            "description": "Optional new due date (YYYY-MM-DD)"
                        },
                        "priority": {
                            "type": "string",
                            "enum": ["P1", "P2", "P3", "P4"],
                            "description": "Optional new priority"
                        }
                    }
                }),
            },
        },
        // 15. create_event — Create a calendar event
        ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "create_event".to_string(),
                description: "Create a new calendar event. Use when the user wants to schedule a meeting, appointment, or event.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "required": ["title"],
                    "properties": {
                        "title": {
                            "type": "string",
                            "description": "Event title"
                        },
                        "is_all_day": {
                            "type": "boolean",
                            "description": "Set to true for all-day events (no specific time). When true, only date is needed in start_at (e.g., '2026-06-20')."
                        },
                        "start_at": {
                            "type": "string",
                            "description": "Start date/time. Use ISO 8601 for timed events (e.g., '2026-06-20T14:00:00') or just a date for all-day events (e.g., '2026-06-20'). Defaults to today if omitted."
                        },
                        "end_at": {
                            "type": "string",
                            "description": "Optional end time in ISO 8601 format"
                        },
                        "location": {
                            "type": "string",
                            "description": "Optional location"
                        },
                        "recurrence": {
                            "type": "string",
                            "enum": ["none", "daily", "weekly", "monthly", "yearly"],
                            "description": "Recurrence pattern. Defaults to 'none' (no repeat)."
                        },
                        "reminders": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Reminder offsets before the event. Use format like '15m' (15 minutes), '1h' (1 hour), '1d' (1 day)."
                        },
                        "content": {
                            "type": "string",
                            "description": "Optional description/notes"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional tags"
                        }
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
pub fn execute_tool(ctx: &ToolContext, name: &str, args: &Value) -> AppResult<String> {
    log::info!("[Syn Tools] Executing tool: {} with args: {}", name, args);

    let result = match name {
        "search_vault" => tool_search_vault(ctx.db, args),
        "get_node" => tool_get_node(ctx.db, args),
        "get_active_tasks_and_events" => tool_get_active_tasks_and_events(ctx.db),
        "get_nodes_by_type" => tool_get_nodes_by_type(ctx.db, args),
        "search_feed_articles" => tool_search_feed_articles(ctx.db, args),
        "get_nodes_by_tag" => tool_get_nodes_by_tag(ctx.db, args),
        "get_linked_nodes" => tool_get_linked_nodes(ctx.db, args),
        "get_all_tags" => tool_get_all_tags(ctx.db),
        "get_node_edges" => tool_get_node_edges(ctx.db, args),
        "search_finance" => tool_search_finance(ctx.db, args),
        "search_files" => tool_search_files(ctx.db, args),
        "create_note" => tool_create_note(ctx, args),
        "create_task" => tool_create_task(ctx, args),
        "update_task_status" => tool_update_task_status(ctx, args),
        "create_event" => tool_create_event(ctx, args),
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

/// 1. search_vault — Universal FTS5 search
fn tool_search_vault(db: &DbBridge, args: &Value) -> AppResult<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: query".to_string()))?;

    let parsed = crate::search::parse_query(query);
    let response = db.search_fts(&parsed, 1, 15)?;

    let results: Vec<Value> = response
        .results
        .iter()
        .map(|r| {
            let snippet: String = r.snippet.chars().take(200).collect();
            serde_json::json!({
                "id": r.id,
                "type": r.item_type,
                "title": r.title,
                "snippet": snippet,
                "tags": r.tags,
                "score": r.score,
            })
        })
        .collect();

    let output = serde_json::json!({
        "results": results,
        "_total": response.total_count,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

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
fn tool_get_active_tasks_and_events(db: &DbBridge) -> AppResult<String> {
    let nodes = db.get_active_tasks_and_events()?;

    let results: Vec<Value> = nodes
        .iter()
        .take(30)
        .map(|n| {
            let status = n
                .properties
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let due_date = n
                .properties
                .get("due_date")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let priority = n
                .properties
                .get("priority")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let start_at = n
                .properties
                .get("start_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            serde_json::json!({
                "id": n.id,
                "title": n.title,
                "type": n.node_type,
                "status": status,
                "due_date": due_date,
                "start_at": start_at,
                "priority": priority,
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

/// 4. get_nodes_by_type — List nodes by type (metadata only)
fn tool_get_nodes_by_type(db: &DbBridge, args: &Value) -> AppResult<String> {
    let node_type = args
        .get("node_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_type".to_string()))?;

    // Validate node type to prevent unexpected queries
    let valid_types = ["note", "task", "event", "person", "quickcap", "file"];
    if !valid_types.contains(&node_type) {
        return Ok(serde_json::json!({
            "error": format!("Invalid node_type '{}'. Must be one of: note, task, event, person, quickcap", node_type)
        })
        .to_string());
    }

    let nodes = db.get_nodes_by_type(node_type)?;

    // Metadata only, no content — limit to 50 items
    let results: Vec<Value> = nodes
        .iter()
        .take(50)
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "title": n.title,
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
fn tool_get_nodes_by_tag(db: &DbBridge, args: &Value) -> AppResult<String> {
    let tag = args
        .get("tag")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: tag".to_string()))?;

    let nodes = db.get_nodes_by_tag(tag)?;

    let results: Vec<Value> = nodes
        .iter()
        .take(30)
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

/// 7. get_linked_nodes — Backlinks for a node
fn tool_get_linked_nodes(db: &DbBridge, args: &Value) -> AppResult<String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".to_string()))?;

    let node_id = args
        .get("node_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

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
fn tool_get_all_tags(db: &DbBridge) -> AppResult<String> {
    let tags = db.get_all_tags_with_counts()?;

    let results: Vec<Value> = tags
        .iter()
        .take(100)
        .map(|(tag, count)| {
            serde_json::json!({
                "tag": tag,
                "count": count,
            })
        })
        .collect();

    let total = tags.len();
    let output = serde_json::json!({
        "results": results,
        "_total": total,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

/// 9. get_node_edges — Knowledge graph edges for a node
fn tool_get_node_edges(db: &DbBridge, args: &Value) -> AppResult<String> {
    let node_id = args
        .get("node_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_id".to_string()))?;

    let edges = db.get_node_edges_for_node(node_id)?;

    let results: Vec<Value> = edges
        .iter()
        .take(30)
        .map(|e| {
            serde_json::json!({
                "source_id": e.source_id,
                "target_id": e.target_id,
                "edge_type": e.edge_type,
                "relation": e.relation,
            })
        })
        .collect();

    let total = edges.len();
    let output = serde_json::json!({
        "results": results,
        "_total": total,
        "_returned": results.len(),
    });

    Ok(output.to_string())
}

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
            let ext = n.properties.get("extension").and_then(|v| v.as_str()).unwrap_or("");
            let size = n.properties.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
            let path = n.properties.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let tags = n.properties.get("tags").cloned().unwrap_or(serde_json::json!([]));
            let people = n.properties.get("people").cloned().unwrap_or(serde_json::json!([]));

            serde_json::json!({
                "id": n.id,
                "filename": n.title,
                "extension": ext,
                "size_bytes": size,
                "path": path,
                "tags": tags,
                "people": people,
                "updated_at": n.updated_at,
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

/// Helper: Create a node file on disk + upsert into DB + update search index.
fn write_tool_node(
    ctx: &ToolContext,
    node_type: &str,
    title: &str,
    content: &str,
    properties: serde_json::Value,
) -> AppResult<(String, String)> {
    let now = chrono::Utc::now();
    let timestamp_str = now.to_rfc3339();
    let timestamp = now.timestamp_millis();

    // Determine subdirectory based on node type
    let subdir = match node_type {
        "task" => "Tasks",
        "event" => "Events",
        _ => "Notes",
    };

    // Sanitize title for filename: remove unsafe characters
    let safe_title: String = title
        .chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' || c == '*' || c == '?' || c == '"' || c == '<' || c == '>' || c == '|' { '_' } else { c })
        .collect();
    let safe_title = safe_title.trim().to_string();
    let rel_path = format!("{}/{}.md", subdir, safe_title);

    // Build YAML frontmatter using serde_yaml (matches CalendarApp's write_node_file pipeline)
    let mut props_map = serde_yaml::Mapping::new();
    props_map.insert(
        serde_yaml::Value::String("title".to_string()),
        serde_yaml::Value::String(title.to_string()),
    );
    props_map.insert(
        serde_yaml::Value::String("type".to_string()),
        serde_yaml::Value::String(node_type.to_string()),
    );

    // Merge all properties (handles Bool, Number, Array, String correctly)
    if let Some(obj) = properties.as_object() {
        for (key, val) in obj {
            if key == "title" || key == "type" || key == "updated_at" {
                continue;
            }
            if let Ok(yaml_val) = serde_yaml::to_value(val) {
                props_map.insert(serde_yaml::Value::String(key.clone()), yaml_val);
            }
        }
    }

    props_map.insert(
        serde_yaml::Value::String("created_at".to_string()),
        serde_yaml::Value::String(timestamp_str.clone()),
    );
    props_map.insert(
        serde_yaml::Value::String("updated_at".to_string()),
        serde_yaml::Value::String(timestamp_str.clone()),
    );

    let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
    let yaml_str = frontmatter.trim_start_matches("---\n");
    let file_content = format!("---\n{}---\n{}", yaml_str, content);

    // Write file to disk
    let full_path = std::path::Path::new(ctx.vault_path).join(&rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&full_path, &file_content)?;

    // Upsert into DB
    let node = crate::models::node::NodeMetadata {
        id: rel_path.clone(),
        node_type: node_type.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        properties: properties.clone(),
        created_at: timestamp_str.clone(),
        updated_at: timestamp_str.clone(),
        timestamp,
        blocks: None,
    };
    ctx.db.upsert_node(&node)?;

    // Update search index
    let tags_str = properties.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let status = properties.get("status").and_then(|v| v.as_str());
    let props_json = serde_json::to_string(&properties).unwrap_or_default();
    ctx.db.upsert_search_entry(
        &rel_path, node_type, title, &tags_str, content,
        &props_json, status, &timestamp_str, &rel_path,
    );

    // Emit event for UI sync
    let _ = ctx.app.emit("node:created", serde_json::json!({
        "id": rel_path,
        "node_type": node_type,
        "title": title,
    }));

    Ok((rel_path, title.to_string()))
}

/// 12. create_note
fn tool_create_note(ctx: &ToolContext, args: &Value) -> AppResult<String> {
    let title = args.get("title").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".into()))?;
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let tags: Vec<String> = args.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let properties = serde_json::json!({ "tags": tags });
    let (id, created_title) = write_tool_node(ctx, "note", title, content, properties)?;

    Ok(serde_json::json!({
        "success": true,
        "id": id,
        "title": created_title,
        "message": format!("Note '{}' created successfully", created_title),
    }).to_string())
}

/// 13. create_task
fn tool_create_task(ctx: &ToolContext, args: &Value) -> AppResult<String> {
    let title = args.get("title").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".into()))?;
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let start_date = args.get("start_date").and_then(|v| v.as_str()).unwrap_or("");
    let due_date = args.get("due_date").and_then(|v| v.as_str()).unwrap_or("");
    let priority = args.get("priority").and_then(|v| v.as_str()).unwrap_or("");
    let tags: Vec<String> = args.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let properties = serde_json::json!({
        "status": "todo",
        "priority": priority,
        "start_date": start_date,
        "due_date": due_date,
        "tags": tags,
    });
    let (id, created_title) = write_tool_node(ctx, "task", title, content, properties)?;

    let mut msg = format!("Task '{}' created (status: todo", created_title);
    if !priority.is_empty() { msg.push_str(&format!(", priority: {}", priority)); }
    if !due_date.is_empty() { msg.push_str(&format!(", due: {}", due_date)); }
    msg.push(')');

    Ok(serde_json::json!({
        "success": true,
        "id": id,
        "title": created_title,
        "message": msg,
    }).to_string())
}

/// 14. update_task_status
fn tool_update_task_status(ctx: &ToolContext, args: &Value) -> AppResult<String> {
    let node_id = args.get("node_id").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: node_id".into()))?;
    let new_status = args.get("status").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: status".into()))?;

    let valid_statuses = ["todo", "in_progress", "done", "canceled", "backlog"];
    if !valid_statuses.contains(&new_status) {
        return Ok(serde_json::json!({
            "error": format!("Invalid status '{}'. Must be one of: {}", new_status, valid_statuses.join(", "))
        }).to_string());
    }

    let node = ctx.db.get_node(node_id)?;
    let Some(mut node) = node else {
        return Ok(serde_json::json!({"error": "Node not found", "node_id": node_id}).to_string());
    };

    if node.node_type != "task" {
        return Ok(serde_json::json!({"error": "Node is not a task", "node_type": node.node_type}).to_string());
    }

    // Update properties
    let mut props = node.properties.clone();
    if let Some(obj) = props.as_object_mut() {
        obj.insert("status".to_string(), serde_json::json!(new_status));

        // Set completed_at when marking done (matches TaskApp behavior)
        if new_status == "done" {
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            obj.insert("completed_at".to_string(), serde_json::json!(today));
        } else {
            // Clear completed_at when un-doing
            obj.insert("completed_at".to_string(), serde_json::json!(""));
        }

        if let Some(due_date) = args.get("due_date").and_then(|v| v.as_str()) {
            if !due_date.is_empty() {
                obj.insert("due_date".to_string(), serde_json::json!(due_date));
            }
        }
        if let Some(priority) = args.get("priority").and_then(|v| v.as_str()) {
            if !priority.is_empty() {
                obj.insert("priority".to_string(), serde_json::json!(priority));
            }
        }
    }
    node.properties = props;
    node.updated_at = chrono::Utc::now().to_rfc3339();
    node.timestamp = chrono::Utc::now().timestamp_millis();

    ctx.db.upsert_node(&node)?;

    // Update file on disk using serde_yaml (matches CalendarApp pipeline)
    let full_path = std::path::Path::new(ctx.vault_path).join(&node.id);
    if full_path.exists() {
        let mut props_map = serde_yaml::Mapping::new();
        props_map.insert(
            serde_yaml::Value::String("title".to_string()),
            serde_yaml::Value::String(node.title.clone()),
        );
        props_map.insert(
            serde_yaml::Value::String("type".to_string()),
            serde_yaml::Value::String("task".to_string()),
        );
        if let Some(obj) = node.properties.as_object() {
            for (key, val) in obj {
                if key == "title" || key == "type" || key == "updated_at" {
                    continue;
                }
                if let Ok(yaml_val) = serde_yaml::to_value(val) {
                    props_map.insert(serde_yaml::Value::String(key.clone()), yaml_val);
                }
            }
        }
        props_map.insert(
            serde_yaml::Value::String("updated_at".to_string()),
            serde_yaml::Value::String(node.updated_at.clone()),
        );
        let frontmatter = serde_yaml::to_string(&props_map).unwrap_or_default();
        let yaml_str = frontmatter.trim_start_matches("---\n");
        let file_content = format!("---\n{}---\n{}", yaml_str, node.content);
        let _ = std::fs::write(&full_path, &file_content);
    }

    // Update search index
    let tags_str = node.properties.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let props_json = serde_json::to_string(&node.properties).unwrap_or_default();
    ctx.db.upsert_search_entry(
        &node.id, "task", &node.title, &tags_str, &node.content,
        &props_json, Some(new_status), &node.updated_at, &node.id,
    );

    // Emit event
    let _ = ctx.app.emit("node:updated", serde_json::json!({
        "id": node.id,
        "node_type": "task",
        "title": node.title,
        "status": new_status,
    }));

    Ok(serde_json::json!({
        "success": true,
        "id": node.id,
        "title": node.title,
        "new_status": new_status,
        "message": format!("Task '{}' status updated to '{}'", node.title, new_status),
    }).to_string())
}

/// 15. create_event
fn tool_create_event(ctx: &ToolContext, args: &Value) -> AppResult<String> {
    let title = args.get("title").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::General("Missing required parameter: title".into()))?;
    let is_all_day = args.get("is_all_day").and_then(|v| v.as_bool()).unwrap_or(false);

    // Default start_at to today if not provided
    let default_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let start_at = args.get("start_at").and_then(|v| v.as_str()).unwrap_or(&default_date);

    // For all-day events, strip time part and auto-set end_at = start_at
    let start_at = if is_all_day {
        start_at.split('T').next().unwrap_or(start_at)
    } else {
        start_at
    };

    let end_at = if is_all_day {
        args.get("end_at").and_then(|v| v.as_str())
            .map(|s| s.split('T').next().unwrap_or(s))
            .unwrap_or(start_at)
    } else {
        args.get("end_at").and_then(|v| v.as_str()).unwrap_or("")
    };

    let location = args.get("location").and_then(|v| v.as_str()).unwrap_or("");
    let recurrence = args.get("recurrence").and_then(|v| v.as_str()).unwrap_or("none");
    let reminders: Vec<String> = args.get("reminders")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let tags: Vec<String> = args.get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let properties = serde_json::json!({
        "is_all_day": is_all_day,
        "start_at": start_at,
        "end_at": end_at,
        "location": location,
        "recurrence": recurrence,
        "reminders": reminders,
        "tags": tags,
    });
    let (id, created_title) = write_tool_node(ctx, "event", title, content, properties)?;

    let time_desc = if is_all_day {
        format!("{} (all day)", start_at)
    } else {
        start_at.to_string()
    };

    Ok(serde_json::json!({
        "success": true,
        "id": id,
        "title": created_title,
        "start_at": start_at,
        "is_all_day": is_all_day,
        "message": format!("Event '{}' created for {}", created_title, time_desc),
    }).to_string())
}

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
//  TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_count() {
        let defs = get_tool_definitions();
        assert_eq!(defs.len(), 15);
    }

    #[test]
    fn test_tool_definitions_have_unique_names() {
        let defs = get_tool_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.function.name.as_str()).collect();

        assert!(names.contains(&"search_vault"));
        assert!(names.contains(&"get_node"));
        assert!(names.contains(&"get_active_tasks_and_events"));
        assert!(names.contains(&"get_nodes_by_type"));
        assert!(names.contains(&"search_feed_articles"));
        assert!(names.contains(&"get_nodes_by_tag"));
        assert!(names.contains(&"get_linked_nodes"));
        assert!(names.contains(&"get_all_tags"));
        assert!(names.contains(&"get_node_edges"));
        assert!(names.contains(&"search_finance"));
        assert!(names.contains(&"search_files"));
        assert!(names.contains(&"create_note"));
        assert!(names.contains(&"create_task"));
        assert!(names.contains(&"update_task_status"));
        assert!(names.contains(&"create_event"));

        // Ensure all names are unique
        let mut unique_names = names.clone();
        unique_names.sort();
        unique_names.dedup();
        assert_eq!(names.len(), unique_names.len());
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
