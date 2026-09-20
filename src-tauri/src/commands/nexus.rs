use crate::db::DbState;
use crate::error::AppResult;
use crate::models::nexus::NexusItem;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub item_type: String,
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct GraphLink {
    pub source: String,
    pub target: String,
}

#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<GraphLink>,
}

#[tauri::command]
pub fn get_nexus_items(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
) -> AppResult<Vec<NexusItem>> {
    let mut items = Vec::new();

    // ─── Query indexed data from SQLite (fast) ─────────────
    {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(rows) = db.get_all_nexus_items() {
            for r in rows {
                if r.item_type == "quickcap"
                    || r.item_type == "message"
                    || r.item_type == "notification"
                {
                    continue;
                }
                if r.path.starts_with("Messages/")
                    || r.path.contains("/Messages/")
                    || r.path.starts_with("Messages\\")
                    || r.path.contains("\\Messages\\")
                    || r.path.starts_with("Syn/")
                    || r.path.starts_with("Syn\\")
                {
                    continue;
                }
                let title = if r.title.is_empty() {
                    match r.item_type.as_str() {
                        "note" => "Untitled Note".to_string(),
                        "task" => "Untitled Task".to_string(),
                        _ => r.title,
                    }
                } else {
                    r.title
                };

                items.push(NexusItem {
                    id: r.id,
                    item_type: r.item_type,
                    title: title.clone(),
                    preview: r.preview,
                    tags: r.tags,
                    date: r.date,
                    path: r.path,
                    content: format!("{} {}", title, r.content),
                    status: r.status,
                });
            }
        }
    }

    items.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(items)
}

#[tauri::command]
pub fn get_nexus_item(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    id: String,
) -> AppResult<NexusItem> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());

    // Fast path: targeted single-table query by ID prefix
    if let Some(r) = db.get_nexus_item_by_id(&id)? {
        let title = if r.title.is_empty() {
            match r.item_type.as_str() {
                "note" => "Untitled Note".to_string(),
                "task" => "Untitled Task".to_string(),
                _ => r.title,
            }
        } else {
            r.title
        };

        return Ok(NexusItem {
            id: r.id,
            item_type: r.item_type,
            title: title.clone(),
            preview: r.preview,
            tags: r.tags,
            date: r.date,
            path: r.path,
            content: format!("{} {}", title, r.content),
            status: r.status,
        });
    }

    // Fallback: full scan (handles edge cases like unexpected ID formats)
    let rows = db.get_all_nexus_items()?;
    for r in rows {
        if r.id == id {
            let title = if r.title.is_empty() {
                match r.item_type.as_str() {
                    "note" => "Untitled Note".to_string(),
                    "task" => "Untitled Task".to_string(),
                    _ => r.title,
                }
            } else {
                r.title
            };

            return Ok(NexusItem {
                id: r.id,
                item_type: r.item_type,
                title: title.clone(),
                preview: r.preview,
                tags: r.tags,
                date: r.date,
                path: r.path,
                content: format!("{} {}", title, r.content),
                status: r.status,
            });
        }
    }
    Err(crate::error::AppError::General(
        "Item not found".to_string(),
    ))
}

/// FTS5-powered universal search across all item types.
/// Supports advanced query syntax: is:, #tag, "phrase", -exclude, in:title, status:, date:
#[tauri::command]
pub fn search_nexus(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
    page: Option<u32>,
    per_page: Option<u32>,
    case_sensitive: Option<bool>,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.case_sensitive = case_sensitive.unwrap_or(false);
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, page.unwrap_or(1), per_page.unwrap_or(50))
}

/// How many matches the graph filter will draw.
///
/// A graph showing more nodes than this is unreadable anyway, and the cap
/// keeps one careless query — `is:note` on a large vault — from serialising
/// the entire index across the IPC boundary to draw a hairball.
const GRAPH_FILTER_LIMIT: u32 = 5000;

/// The ids a query matches, for filtering the Nexus graph.
///
/// The graph wants the set of matches, not the ranked, snippet-bearing results
/// the search panel shows. It still goes through `search_fts` rather than
/// growing a second query builder beside it: the filter semantics here
/// (`is:`, `#tag`, `status:`, the property lookups) have been subtly wrong
/// before, and two implementations would be two chances to be wrong
/// differently. Ranking and snippets are built and then dropped, which is the
/// price — and not entirely waste, since the case-sensitive post-filter reads
/// the snippet to decide what stays.
#[tauri::command]
pub fn search_nexus_ids(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
    case_sensitive: Option<bool>,
) -> AppResult<Vec<String>> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.case_sensitive = case_sensitive.unwrap_or(false);
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    let response = db.search_fts(&parsed, 1, GRAPH_FILTER_LIMIT)?;
    Ok(response.results.into_iter().map(|r| r.id).collect())
}

/// FTS5-powered search scoped to notes only.
/// Used by the Note mini-app sidebar search.
#[tauri::command]
pub fn search_notes(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    // Force type filter to "note" regardless of user input
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("note".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 100)
}

/// FTS5-powered search scoped to tasks only.
/// Used by the Task mini-app search.
#[tauri::command]
pub fn search_tasks(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("task".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 200)
}

/// FTS5-powered search scoped to files only.
/// Used by the File Manager mini-app search.
#[tauri::command]
pub fn search_files(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("file".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 200)
}

/// FTS5-powered search scoped to quickcaps only.
/// Used by the QuickCap mini-app search bar.
///
/// Quickcaps are deliberately absent from the Nexus item list and the graph —
/// they are fleeting notes, not knowledge — but they are still indexed, so
/// they stay findable. This is the scoped entry point that makes that true.
#[tauri::command]
pub fn search_quickcaps(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
    query: String,
) -> AppResult<crate::search::SearchResponse> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.type_filter = Some("quickcap".to_string());
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.search_fts(&parsed, 1, 200)
}

#[tauri::command]
pub fn get_nexus_graph_data(
    _app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    _vault_path: String,
) -> AppResult<GraphData> {
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    graph_data(&db)
}

/// The graph, from a plain connection so a test can build one.
pub(crate) fn graph_data(db: &crate::db::DbBridge) -> AppResult<GraphData> {
    let items = db.get_all_nexus_items()?;
    let node_edges = db.get_all_node_edges()?;

    // Edges name their ends by stable identity, a node's `node_id`, so that a
    // link survives its file moving. The graph names nodes by path. Comparing
    // the two directly dropped every edge out of a node that had an identity:
    // on the vault this was measured on, 202 of 215 edges never reached Nexus.
    let path_of = db.paths_by_stable_id()?;

    let mut nodes = Vec::new();
    let mut links = Vec::new();

    let mut node_ids = std::collections::HashSet::new();
    let mut tag_nodes = HashMap::new();
    let mut ghost_nodes = HashMap::new();
    let mut added_links = std::collections::HashSet::new();

    // 1. Build graph nodes from items
    for r in &items {
        if r.item_type == "quickcap" || r.item_type == "message" || r.item_type == "notification" {
            continue;
        }
        if r.path.starts_with("Messages/")
            || r.path.contains("/Messages/")
            || r.path.starts_with("Messages\\")
            || r.path.contains("\\Messages\\")
            || r.path.starts_with("Syn/")
            || r.path.starts_with("Syn\\")
        {
            continue;
        }

        let title = if r.title.is_empty() {
            match r.item_type.as_str() {
                "note" => "Untitled Note".to_string(),
                "task" => "Untitled Task".to_string(),
                _ => r.title.clone(),
            }
        } else {
            r.title.clone()
        };

        node_ids.insert(r.id.clone());
        nodes.push(GraphNode {
            id: r.id.clone(),
            item_type: r.item_type.clone(),
            title,
            tags: r.tags.clone(),
        });

        // Tag nodes from properties (not from edges)
        for mut tag in r.tags.clone() {
            if tag.starts_with("#") {
                tag = tag[1..].to_string();
            }
            let tag_clean = tag.trim().to_lowercase();
            if tag_clean.is_empty() {
                continue;
            }

            let tag_id = format!("tag-{}", tag_clean);
            if !tag_nodes.contains_key(&tag_id) {
                tag_nodes.insert(
                    tag_id.clone(),
                    GraphNode {
                        id: tag_id.clone(),
                        item_type: "tag".to_string(),
                        title: format!("#{}", tag_clean),
                        tags: vec![],
                    },
                );
            }

            let link_key = format!("{}->{}", r.id, tag_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key);
                links.push(GraphLink {
                    source: r.id.clone(),
                    target: tag_id,
                });
            }
        }
    }

    // 2. Build links from node_edges (already ID-based — no resolution needed)
    for edge in node_edges {
        let source_id = path_of
            .get(&edge.source_id)
            .cloned()
            .unwrap_or_else(|| edge.source_id.clone());
        // Skip edges where source is not in our graph
        if !node_ids.contains(&source_id) {
            continue;
        }

        // Handle ghost targets
        let target_id = if edge.target_id.starts_with("ghost:") {
            let ghost_title = edge
                .target_id
                .strip_prefix("ghost:")
                .unwrap_or(&edge.target_id);
            let ghost_id = format!("ghost-{}", ghost_title);
            if !ghost_nodes.contains_key(&ghost_id) {
                ghost_nodes.insert(
                    ghost_id.clone(),
                    GraphNode {
                        id: ghost_id.clone(),
                        item_type: "ghost".to_string(),
                        title: ghost_title.to_string(),
                        tags: vec![],
                    },
                );
            }
            ghost_id
        } else {
            match path_of.get(&edge.target_id).cloned() {
                Some(path) if node_ids.contains(&path) => path,
                _ if node_ids.contains(&edge.target_id) => edge.target_id.clone(),
                _ => continue, // Target node doesn't exist and isn't a ghost — skip
            }
        };

        if target_id != source_id {
            let link_key = format!("{}->{}", source_id, target_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key);
                links.push(GraphLink {
                    source: source_id,
                    target: target_id,
                });
            }
        }
    }

    for (_, tag_node) in tag_nodes {
        nodes.push(tag_node);
    }
    for (_, ghost_node) in ghost_nodes {
        nodes.push(ghost_node);
    }

    Ok(GraphData { nodes, links })
}

/// Run a saved query and return the notes it matches, with the columns it
/// asked for.
///
/// Separate from `search_nexus` because the two answer different questions:
/// search asks which notes mention some words, a query asks which notes *are*
/// something — every task still open, every note over budget. Only the second
/// reads frontmatter as data and returns columns.
/// What types this vault contains, and which fields each one uses.
///
/// Deliberately unfiltered. `run_node_query` hides `finance_%` because those
/// nodes are a storage detail rather than something anyone browses, but this
/// command answers "what is in the vault", and hiding part of the answer here
/// would make two callers disagree about what exists. Whoever displays it
/// decides what to show.
#[tauri::command]
pub fn list_observed_types(
    state: tauri::State<'_, DbState>,
) -> AppResult<Vec<crate::models::node::ObservedType>> {
    // Enough keys to describe a type, few enough that one node carrying a
    // large generated blob cannot crowd out every other type in the answer.
    const KEYS_PER_TYPE: usize = 25;

    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    Ok(db
        .observed_schemas(KEYS_PER_TYPE)?
        .into_iter()
        .map(
            |(node_type, count, fields)| crate::models::node::ObservedType {
                node_type,
                count,
                fields: fields
                    .into_iter()
                    .map(|(key, count, sample)| crate::models::node::ObservedField {
                        key,
                        count,
                        sample,
                    })
                    .collect(),
            },
        )
        .collect())
}

/// Run one question, against whichever table can answer it.
///
/// One command rather than two, because a person asking a question does not
/// know which table holds the answer and should not have to. The words decide:
/// `with:`, `where:`, `about:`, `when:`, `shape:` and `magnitude:` are only
/// answerable of an event, so writing one of them is the same act as naming
/// the timeline. Everything else means what it has always meant, so no call
/// that worked before this changes.
///
/// Both halves return the same `QueryResult`, which is what lets one view draw
/// either — see `timeline::query`.
#[tauri::command]
pub fn run_node_query(
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, crate::timeline::TimelineState>,
    vault_path: Option<String>,
    query: String,
    // Rows to skip. Absent means the first page, which is every existing call.
    offset: Option<u32>,
) -> AppResult<crate::db::QueryResult> {
    let mut asked = crate::query::parse(&query);
    asked.offset = offset.unwrap_or(0);
    answer(&state, &timeline, vault_path.as_deref(), &asked, None)
}

/// One question answered, whichever table answers it.
///
/// Both commands come through here, which is the only reason they cannot
/// answer the same question two different ways. The one thing they differ in
/// is the last argument: whether there is anywhere to spend money.
fn answer(
    state: &tauri::State<'_, DbState>,
    timeline: &tauri::State<'_, crate::timeline::TimelineState>,
    vault_path: Option<&str>,
    asked: &crate::query::Query,
    asker: Option<&dyn crate::pipeline::Asks>,
) -> AppResult<crate::db::QueryResult> {
    let today = chrono::Local::now().date_naive();

    if asked.source_of() == crate::query::Source::Nodes {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        let found = db.run_node_query(asked)?;
        let words = VaultWords::of(&db, vault_path.unwrap_or(""))?;
        return crate::pipeline::run_around(
            asked,
            found,
            &crate::pipeline::Around { today, words: Some(&words), asker },
        );
    }

    // A name becomes an identity here, where the vault can be read: an event's
    // links name people by identity, and a person typing a question names them
    // by their name. Gathered from the whole tree rather than from three lists,
    // because a name can now sit inside a bracket or under a `NOT`.
    let named = {
        let db = state.lock().unwrap_or_else(|e| e.into_inner());
        crate::timeline::query::Named(
            crate::timeline::query::names_in(&asked.filter)
                .into_iter()
                .map(|name| {
                    let identity = identity_of(&db, &name);
                    (name, identity)
                })
                .collect(),
        )
    };

    let mut store = timeline.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(vault_path) = vault_path {
        crate::timeline::store::catch_up_in(state.inner(), &mut store, Some(vault_path))?;
    }
    let mut found = crate::timeline::query::run(&store, asked, &named)?;

    // One place for both sources: the pipeline works on rows, and by here the
    // rows are rows whichever table they came out of. That is the same claim
    // `QueryResult` has been making since the lenses went in.
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    name_the_people(&db, &mut found);
    let words = VaultWords::of(&db, vault_path.unwrap_or(""))?;
    crate::pipeline::run_around(
        asked,
        found,
        &crate::pipeline::Around { today, words: Some(&words), asker },
    )
}

/// The vault's words, with everything withheld already gone.
///
/// This is where the consent layer meets the query language, and the order is
/// the whole of it: a sealed note, a sealed period, a hushed day, a hushed
/// moment and a hushed sentence are dropped **before** anything is built out
/// of them. Not filtered from the answer — never read. `timeline::year`
/// established the rule ("so it was not merely left out of the answer — it was
/// never sent") and a lens that can reach the same sentences has to keep it,
/// or the language is a hole in the layer above it.
pub(crate) struct VaultWords<'a> {
    db: &'a crate::db::DbBridge,
    seals: std::sync::Arc<crate::timeline::seal::Seals>,
    quiet: std::sync::Arc<crate::timeline::quiet::Quiet>,
}

/// The most sentences taken from any one note.
///
/// The same three as `timeline::year`: a long note would otherwise fill the
/// list on its own, and what is wanted is a spread across days rather than the
/// whole of the wordiest one.
const MOST_PER_NOTE: usize = 3;

impl<'a> VaultWords<'a> {
    pub(crate) fn of(db: &'a crate::db::DbBridge, vault_path: &str) -> AppResult<VaultWords<'a>> {
        Ok(VaultWords {
            db,
            seals: crate::timeline::seal::current(db, vault_path)?,
            quiet: crate::timeline::quiet::current(db, vault_path)?,
        })
    }
}

impl crate::pipeline::Words for VaultWords<'_> {
    fn sentences_of(&self, node: &str, day: &str) -> Vec<String> {
        // Every gate the year feature passes through, in the same order.
        let withheld = self.seals.hides(node)
            || (!day.is_empty() && self.seals.covers(day))
            || (!day.is_empty() && self.quiet.hushes_day(day))
            || (!day.is_empty() && self.quiet.hushes_moment(node, day));
        if withheld {
            return Vec::new();
        }
        let Ok(content) = self.db.conn().query_row(
            "SELECT COALESCE(content, '') FROM nodes WHERE id = ?1 OR stable_id = ?1",
            rusqlite::params![node],
            |row| row.get::<_, String>(0),
        ) else {
            return Vec::new();
        };
        crate::timeline::onthisday::sentences(&content)
            .into_iter()
            .filter(|text| !self.quiet.hushes_line(node, text))
            .take(MOST_PER_NOTE)
            .collect()
    }
}

/// Put names back where the links left identities.
///
/// The way in resolves a name to an identity (`identity_of`); this is the way
/// back, and it belongs here for the same reason — the vault is readable here
/// and not inside the timeline's own connection.
///
/// Before the pipeline, deliberately. `seq gaps by who` gathers rows under
/// whatever is in that column, and a silence gathered under `uuid-khanh` is
/// not the silence panel it was supposed to replace: it is the same answer
/// written in a language nobody speaks.
fn name_the_people(db: &crate::db::DbBridge, result: &mut crate::db::QueryResult) {
    let Some(at) = result.columns.iter().position(|c| c == "who") else {
        return;
    };
    let ids: Vec<String> = result
        .rows
        .iter()
        .filter_map(|row| row.cells.get(at))
        .flat_map(|cell| cell.split(',').map(|id| id.trim().to_string()))
        .filter(|id| !id.is_empty())
        .collect();
    let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
    let names = crate::timeline::store::names_for(db, &borrowed);
    for row in &mut result.rows {
        if let Some(cell) = row.cells.get_mut(at) {
            *cell = cell
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                // A place that is only words somebody typed has no node and no
                // title, so it answers for itself and is left as written.
                .map(|id| names.get(id).cloned().unwrap_or_else(|| id.to_string()))
                .collect::<Vec<_>>()
                .join(", ");
        }
    }
}

/// Running a question that spends money, because somebody asked it to.
///
/// A separate command rather than a flag on `run_node_query`, and that is the
/// guardrail of §13.3 rather than a nicety:
///
/// 1. **Opening a lens cannot spend.** The ordinary path has no asker, so a
///    saved question carrying `| ask` refuses there — with the price in the
///    refusal, which is the preview.
/// 2. **The screen can tell them apart.** Two doors, so the button that costs
///    money can say so and look different from the one that does not.
///
/// Everything sealed or hushed is dropped before the prompt is built, so it is
/// not merely left out of the answer — it was never sent. That rule comes from
/// `timeline::year` and is enforced here by `VaultWords`, which is the only
/// way this command can read a sentence at all.
#[tauri::command(async)]
pub async fn ask_node_query(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, DbState>,
    timeline: tauri::State<'_, crate::timeline::TimelineState>,
    vault_path: String,
    query: String,
    offset: Option<u32>,
) -> AppResult<crate::db::QueryResult> {
    let mut asked = crate::query::parse(&query);
    asked.offset = offset.unwrap_or(0);
    let spends = asked
        .stages
        .iter()
        .any(|stage| matches!(stage, crate::query::Stage::Ask(_)));
    if !spends {
        // Nothing here costs anything, so there is no reason to be on this
        // door. Answered the same way rather than differently, which is what
        // the shared `answer` is for.
        return answer(&state, &timeline, Some(&vault_path), &asked, None);
    }

    // Everything the model half needs, settled before anything is sent.
    let settings = crate::commands::syn::settings_for(&vault_path);
    if !settings.enabled {
        return Err(crate::error::AppError::General(
            crate::commands::syn::SWITCHED_OFF.into(),
        ));
    }
    let config = crate::timeline::extract::read_config(&vault_path);
    let (settings, model) = crate::timeline::extract::reader(&config, &settings);
    let Some(model) = model else {
        return Err(crate::error::AppError::General("No model is configured".into()));
    };
    if !crate::timeline::media::runs_here(&settings) && !config.allow_cloud {
        return Err(crate::error::AppError::General(
            "Your own sentences are read only by a model on this machine, unless \
             sending them elsewhere is allowed for this vault"
                .into(),
        ));
    }

    // The rows, up to the point where the money would be spent. The same
    // stages, run the same way as on the free path — the only difference is
    // that this time there is somewhere to spend.
    let at = asked
        .stages
        .iter()
        .position(|stage| matches!(stage, crate::query::Stage::Ask(_)))
        .expect("a stage that was just found");
    let crate::query::Stage::Ask(room) = asked.stages[at] else {
        unreachable!("the stage at the position of an ask is an ask")
    };
    if asked.stages[at + 1..]
        .iter()
        .any(|stage| matches!(stage, crate::query::Stage::Ask(_)))
    {
        return Err(crate::error::AppError::Refused(
            crate::refusal::Refusal::ask_only_once(),
        ));
    }

    let before = crate::query::Query {
        stages: asked.stages[..at].to_vec(),
        ..asked.clone()
    };
    let found = answer(&state, &timeline, Some(&vault_path), &before, None)?;

    // Short enough to need no choosing, and asking would cost a call to be
    // told the same list back. `timeline::year` learnt this first.
    if found.rows.len() <= room as usize {
        return Ok(found);
    }

    let lines = crate::pipeline::lines_of(&found);
    let prompt = crate::timeline::year::prompt_for(&lines, room as usize);
    let provider = crate::commands::syn::provider_for(&app_handle, &settings).await;
    let reply = provider
        .chat(crate::syn::provider::ChatRequest {
            model: &model,
            messages: &[crate::syn::provider::ChatMessage::new("user", prompt)],
            temperature: Some(0.0),
            num_ctx: settings.num_ctx,
            tools: None,
        })
        .await?;
    let picked = crate::timeline::year::parse_reply(&reply.content).ok_or_else(|| {
        crate::error::AppError::General("the reply was not the JSON asked for".into())
    })?;

    let kept = crate::pipeline::keeping_picked(found, &picked, room as usize);

    // And whatever was asked for after the ask.
    let after = crate::query::Query {
        stages: asked.stages[at + 1..].to_vec(),
        ..asked
    };
    crate::pipeline::run_on(&after, kept, chrono::Local::now().date_naive())
}

/// What an event's links would call this name.
///
/// A path, a stable id or a person's name all reach the same node; the links
/// hold whichever of its names the vault gave it. A name that resolves to
/// nothing is passed through untouched, so a query naming an identity directly
/// still works and a typo simply finds nothing instead of finding everything.
fn identity_of(db: &crate::db::DbBridge, name: &str) -> String {
    let Some(id) = crate::timeline::store::node_for(db, name) else {
        return name.to_string();
    };
    db.conn()
        .query_row(
            "SELECT COALESCE(NULLIF(json_extract(properties, '$.node_id'), ''), \
                             NULLIF(stable_id, ''), id)
               FROM nodes WHERE id = ?1",
            [&id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or(id)
}

#[cfg(test)]
mod things_gate {
    use crate::db::DbBridge;

    /// Gate T1: a type nobody coded for reaches the screen from a plain file.
    ///
    /// The whole claim of Things in one test. Somebody writes a markdown file
    /// with `type: animal` in the frontmatter — in this app, in Obsidian, in
    /// vim — and without a registration step, a manifest, or a line of code:
    ///
    /// 1. the scan indexes it as `animal`, not as a note,
    /// 2. `list_observed_types` reports the type and the fields it carries,
    ///    which is what the left rail is drawn from,
    /// 3. `run_node_query` returns it for `type:animal`, which is the list.
    ///
    /// Each of those three has failed before. The scan is what `NodeType::Other`
    /// protects; the rail could have been a list in the code; and `type:animal`
    /// returned the entire vault until the query parser stopped recognising
    /// exactly five type names.
    /// A node the assistant made is the same kind of object as one the app made.
    ///
    /// They used to be written by two different functions. The assistant's one
    /// wrote the file, upserted the row and indexed the text; the app's one
    /// also registered the vault identity, assigned the node's `node_id` and
    /// wrote it into the frontmatter, recorded that id against the path, and
    /// handed the content to the CRDT bridge.
    ///
    /// `node_id` is the one to check. It is what the sync engine calls the
    /// file, it is what edges are recorded against, and it is what lets a node
    /// keep its links through a rename — a file created without one has its
    /// identity decided later by whatever reaches it first.
    #[test]
    fn a_node_the_assistant_creates_is_written_the_way_the_app_writes_one() {
        let dir = tempfile::tempdir().expect("temp vault");
        // Canonicalised, and the reason is worth knowing. `/var` on macOS is a
        // symlink to `/private/var`, so a temporary directory hands back a path
        // that the write path's own `resolve_safe_path` then resolves to a
        // different string. A node's id is its path relative to the vault, and
        // it is worked out by stripping the vault prefix — which fails when the
        // two spellings disagree, leaving the node indexed under an absolute
        // path. That is a real fragility for any vault reached through a
        // symlink, and it is older than this test; pinned here so the test is
        // measuring the writer rather than the tempdir.
        let vault = dir.path().canonicalize().expect("canonical temp vault");
        let vault = vault.as_path();

        let db = DbBridge::new_in_memory_full().expect("schema");
        let app = tauri::test::mock_builder()
            .manage(std::sync::Mutex::new(db))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let handle = app.handle().clone();
        let db_state = tauri::Manager::state::<crate::db::DbState>(&handle);
        let vault_path = vault.to_string_lossy().to_string();

        let ctx = crate::syn::tools::ToolContext {
            db: db_state.inner(),
            vault_path: &vault_path,
            app: &handle,
            // Not part of a run: this is a screen calling a tool directly.
            run_id: None,
        };

        crate::syn::tools::execute_tool(
            &ctx,
            "create_node",
            &serde_json::json!({
                "node_type": "animal",
                "title": "Mèo Mun",
                "properties": { "species": "mèo" },
            }),
        )
        .expect("the tool runs");

        // The folder rule, which both writers now share.
        let written = vault.join("Animal/Mèo Mun.md");
        assert!(written.exists(), "an animal belongs in Animal/, not Notes/");

        let on_disk = std::fs::read_to_string(&written).expect("readable");
        assert!(on_disk.contains("type: animal"));
        assert!(on_disk.contains("species: mèo"));

        // The part that was missing. Written into the file by the shared path,
        // not left for a later scan to guess at.
        assert!(
            on_disk.contains("node_id:"),
            "a node created by the assistant has no identity:\n{on_disk}"
        );

        let db = db_state.lock().expect("lock");
        let indexed = db
            .get_node("Animal/Mèo Mun.md")
            .expect("query")
            .expect("the node is in the index");
        assert_eq!(indexed.node_type, "animal");
        assert!(indexed.properties.get("node_id").is_some());
    }

    /// Gate T3: a field somebody typed into a file can be worked with.
    ///
    /// "Custom fields" means nothing unless you can do something with one, so
    /// this exercises all four operations against a key that exists only
    /// because it was written into two files by hand:
    ///
    /// - filter on it,
    /// - sort on it,
    /// - read it back as a column, which is what grouping needs,
    /// - and count what matched, ignoring the page size.
    ///
    /// All four are the engine's work rather than the browser's. Filtering a
    /// page after it arrives would make `total` a lie and sorting a page would
    /// sort the wrong rows.
    #[test]
    fn a_field_nobody_declared_can_be_filtered_sorted_and_shown() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path();
        std::fs::create_dir_all(vault.join("Tasks")).expect("mkdir");

        for (file, title, energy) in [
            ("a.md", "Viết changelog", "low"),
            ("b.md", "Dọn log cũ", "low"),
            ("c.md", "Thiết kế lại trang giá", "high"),
        ] {
            std::fs::write(
                vault.join("Tasks").join(file),
                format!("---\ntitle: {title}\ntype: task\nstatus: todo\nenergy: {energy}\n---\n"),
            )
            .expect("write");
        }
        // One without the field at all, which must not vanish from a query
        // that does not mention it.
        std::fs::write(
            vault.join("Tasks/d.md"),
            "---\ntitle: Gia hạn tên miền\ntype: task\nstatus: todo\n---\n",
        )
        .expect("write");

        let db = DbBridge::new_in_memory_full().expect("schema");
        let app = tauri::test::mock_builder()
            .manage(std::sync::Mutex::new(db))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let handle = app.handle().clone();
        let db_state = tauri::Manager::state::<crate::db::DbState>(&handle);
        crate::commands::nodes::scan_vault_into_db(&handle, db_state.inner(), &vault.to_string_lossy())
            .expect("the vault scans");

        let db = db_state.lock().expect("lock");
        let ask = |q: &str| db.run_node_query(&crate::query::parse(q)).expect("query runs");

        // The menus are built from this, so it has to see the field first.
        let observed = db.observed_schemas(25).expect("schemas");
        let tasks = observed.iter().find(|(t, ..)| t == "task").expect("task");
        assert!(
            tasks.2.iter().any(|(k, ..)| k == "energy"),
            "{:?}",
            tasks.2
        );

        // Filter.
        assert_eq!(ask("type:task energy:low").total, 2);
        assert_eq!(ask("type:task energy:high").total, 1);
        // And the negation, which is what "everything unfinished" needs.
        assert_eq!(ask("type:task -energy:low").total, 2, "high, plus the one with no energy");

        // Sort, on a frontmatter key the engine was never taught.
        let sorted = ask("type:task energy:low sort:title columns:energy");
        let titles: Vec<&str> = sorted.rows.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, vec!["Dọn log cũ", "Viết changelog"]);

        // The column comes back, which is both how it is displayed and how the
        // list groups: `QueryRow.cells` holds only the columns that were asked
        // for, so a group key has to be requested to exist at all.
        let at = sorted
            .columns
            .iter()
            .position(|c| c == "energy")
            .expect("`energy` is a column the engine returned");
        assert!(sorted.rows.iter().all(|r| r.cells[at] == "low"));

        // The count survives a page. This is the shape that once reported two
        // tasks out of a hundred and twenty-six.
        let one = ask("type:task limit:1");
        assert_eq!(one.rows.len(), 1);
        assert_eq!(one.total, 4);
    }

    #[test]
    fn a_type_nobody_coded_for_reaches_the_rail_and_the_list() {
        let dir = tempfile::tempdir().expect("temp vault");
        let vault = dir.path();

        std::fs::create_dir_all(vault.join("Animal")).expect("mkdir");
        std::fs::write(
            vault.join("Animal/meo-mun.md"),
            "---\ntitle: Mèo Mun\ntype: animal\nspecies: mèo\ncolour: đen\n---\nNhặt được ở ngõ.\n",
        )
        .expect("write");
        std::fs::write(
            vault.join("Animal/cho-vang.md"),
            "---\ntitle: Chó Vàng\ntype: animal\nspecies: chó\nvaccinated_at: 2026-06-12\n---\n",
        )
        .expect("write");
        std::fs::create_dir_all(vault.join("Notes")).expect("mkdir");
        std::fs::write(
            vault.join("Notes/a.md"),
            "---\ntitle: Ghi chú\ntype: note\n---\nnội dung\n",
        )
        .expect("write");

        let db = DbBridge::new_in_memory_full().expect("schema");
        let app = tauri::test::mock_builder()
            .manage(std::sync::Mutex::new(db))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock app");
        let handle = app.handle().clone();
        let db_state = tauri::Manager::state::<crate::db::DbState>(&handle);
        crate::commands::nodes::scan_vault_into_db(&handle, db_state.inner(), &vault.to_string_lossy())
            .expect("the vault scans");

        let db = db_state.lock().expect("lock");

        // 1. Indexed as what the file says, not as the folder or a fallback.
        let observed = db.observed_schemas(25).expect("schemas");
        let animals = observed
            .iter()
            .find(|(t, ..)| t == "animal")
            .expect("`animal` is a type this vault has, whatever the code knows");
        assert_eq!(animals.1, 2);

        // 2. The fields the rail and the arrangement menus read. The union
        //    across nodes, not the intersection: `colour` is on one animal and
        //    `vaccinated_at` on the other, and both are real fields of this
        //    vault's animals.
        //
        //    `title` and `type` are in here too, because they are frontmatter
        //    like everything else — the scan does not strip them out, and a
        //    query can sort on either. Whoever builds a menu decides whether to
        //    offer them; this reports what the file holds.
        let mut fields: Vec<String> = animals.2.iter().map(|(k, ..)| k.clone()).collect();
        fields.sort();
        assert_eq!(
            fields,
            vec!["colour", "species", "title", "type", "vaccinated_at"]
        );

        // 3. The list. `type:` rather than `is:` because that is what the app
        //    and the assistant both write.
        let found = db
            .run_node_query(&crate::query::parse("type:animal"))
            .expect("query runs");
        assert_eq!(found.total, 2, "both animals, and nothing else");

        let mut titles: Vec<&str> = found.rows.iter().map(|r| r.title.as_str()).collect();
        titles.sort();
        assert_eq!(titles, vec!["Chó Vàng", "Mèo Mun"]);
        assert!(found.rows.iter().all(|r| r.node_type == "animal"));

        // And the note is not swept in, which is the failure mode a dropped
        // type filter used to produce.
        assert!(!titles.contains(&"Ghi chú"));
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use crate::db::{DbBridge, NodeEdge};
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    fn node(id: &str, node_type: &str, title: &str, properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: title.to_string(),
            content: String::new(),
            properties,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            updated_at: "2026-01-01T00:00:00.000Z".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    fn edge(source: &str, target: &str, edge_type: &str) -> NodeEdge {
        NodeEdge {
            id: format!("{source}->{target}"),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: edge_type.to_string(),
            relation: None,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
        }
    }

    /// Edges are stored under a node's identity and the graph names nodes by
    /// path. On a real vault, 202 of 215 edges were lost between the two.
    #[test]
    fn a_link_between_nodes_with_identities_reaches_the_graph() {
        let db = DbBridge::new_in_memory_full().unwrap();
        db.upsert_node(&node("Notes/a.md", "note", "A", json!({ "node_id": "uuid-a" }))).unwrap();
        db.upsert_node(&node("People/b.md", "person", "B", json!({ "node_id": "uuid-b" }))).unwrap();
        db.upsert_node(&node("Notes/old.md", "note", "Old", json!({}))).unwrap();
        db.upsert_node_edge(&edge("uuid-a", "uuid-b", "person_link")).unwrap();
        db.upsert_node_edge(&edge("Notes/old.md", "uuid-a", "wikilink")).unwrap();
        db.upsert_node_edge(&edge("uuid-a", "ghost:Nowhere", "wikilink")).unwrap();

        let graph = graph_data(&db).unwrap();
        let has = |s: &str, t: &str| graph.links.iter().any(|l| l.source == s && l.target == t);

        assert!(has("Notes/a.md", "People/b.md"), "identity to identity");
        assert!(has("Notes/old.md", "Notes/a.md"), "a path to an identity");
        assert!(has("Notes/a.md", "ghost-Nowhere"), "an identity to an unresolved link");
    }
}

#[cfg(test)]
mod consent_gate {
    //! The one thing step 7 must not get wrong.
    //!
    //! `| explode sentences` is the first part of the query language that can
    //! reach the **words a person wrote**, rather than the titles and dates an
    //! index holds. Everything above it — seals, hushes — exists to decide
    //! what the app may look at. A language that can reach round that is not a
    //! feature with a bug in it; it is a hole in the layer, and the layer is
    //! the reason any of this is allowed near a diary.
    //!
    //! So: written down as a test, not as a comment.

    use super::VaultWords;
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use crate::pipeline::Words;
    use crate::timeline::quiet::{self, Subject};

    const DAY: &str = "2019-11-05";
    const WORDS: &str = "Hôm nay gặp Khánh ở quán quen. Nói chuyện rất lâu về mọi thứ.";

    fn vault_with(properties: serde_json::Value) -> (tempfile::TempDir, DbBridge) {
        let dir = tempfile::tempdir().expect("a temp vault");
        let db = DbBridge::new_in_memory_full().expect("a database");
        db.upsert_node(&NodeMetadata {
            id: "Notes/diary.md".into(),
            node_type: "note".into(),
            title: DAY.into(),
            content: WORDS.into(),
            properties,
            created_at: "2019-11-05T00:00:00.000Z".into(),
            updated_at: "2019-11-05T00:00:00.000Z".into(),
            timestamp: 0,
            blocks: None,
        })
        .expect("seed");
        (dir, db)
    }

    fn read(dir: &tempfile::TempDir, db: &DbBridge) -> Vec<String> {
        VaultWords::of(db, dir.path().to_str().expect("a path"))
            .expect("the consent layer loads")
            .sentences_of("Notes/diary.md", DAY)
    }

    /// The control. Without it the three tests below would pass on a bug that
    /// returned nothing for everything.
    #[test]
    fn an_ordinary_note_gives_up_its_sentences() {
        let (dir, db) = vault_with(serde_json::json!({ "date": DAY }));
        assert_eq!(read(&dir, &db).len(), 2, "{:?}", read(&dir, &db));
    }

    #[test]
    fn a_sealed_note_gives_up_nothing() {
        let (dir, db) = vault_with(serde_json::json!({ "date": DAY, "sealed": true }));
        assert!(read(&dir, &db).is_empty());
    }

    #[test]
    fn a_note_inside_a_sealed_period_gives_up_nothing() {
        let (dir, db) = vault_with(serde_json::json!({ "date": DAY }));
        let vault = dir.path().to_str().expect("a path");
        crate::timeline::seal::write_period(vault, "2019-11-01", "2019-11-30")
            .expect("the period is sealed");
        assert!(read(&dir, &db).is_empty());
    }

    /// A hush is quieter than a seal — the app simply does not raise it first
    /// — and one sentence can be hushed on its own. Both have to hold here.
    #[test]
    fn a_hushed_sentence_is_left_behind_and_the_rest_are_not() {
        let (dir, db) = vault_with(serde_json::json!({ "date": DAY }));
        let vault = dir.path().to_str().expect("a path");
        quiet::write_hush(
            vault,
            &Subject::Line {
                node: "Notes/diary.md".into(),
                line: quiet::line_id("Hôm nay gặp Khánh ở quán quen."),
            },
            None,
        )
        .expect("the line is hushed");

        let left = read(&dir, &db);
        assert_eq!(left.len(), 1, "{left:?}");
        assert!(!left[0].contains("Khánh"), "{left:?}");
    }

    #[test]
    fn a_hushed_stretch_of_time_gives_up_nothing() {
        let (dir, db) = vault_with(serde_json::json!({ "date": DAY }));
        let vault = dir.path().to_str().expect("a path");
        quiet::write_hush(
            vault,
            &Subject::Period { from: "2019-11-01".into(), to: "2019-11-30".into() },
            None,
        )
        .expect("the stretch is hushed");
        assert!(read(&dir, &db).is_empty());
    }
}
