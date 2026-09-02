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
    let items = db.get_all_nexus_items()?;
    let node_edges = db.get_all_node_edges()?;

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
        // Skip edges where source is not in our graph
        if !node_ids.contains(&edge.source_id) {
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
        } else if !node_ids.contains(&edge.target_id) {
            continue; // Target node doesn't exist and isn't a ghost — skip
        } else {
            edge.target_id.clone()
        };

        if target_id != edge.source_id {
            let link_key = format!("{}->{}", edge.source_id, target_id);
            if !added_links.contains(&link_key) {
                added_links.insert(link_key);
                links.push(GraphLink {
                    source: edge.source_id,
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

#[tauri::command]
pub fn run_node_query(
    state: tauri::State<'_, DbState>,
    query: String,
    // Rows to skip. Absent means the first page, which is every existing call.
    offset: Option<u32>,
) -> AppResult<crate::db::QueryResult> {
    let mut parsed = crate::search::parse_query(&query);
    parsed.offset = offset.unwrap_or(0);
    let db = state.lock().unwrap_or_else(|e| e.into_inner());
    db.run_node_query(&parsed)
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
        let ask = |q: &str| db.run_node_query(&crate::search::parse_query(q)).expect("query runs");

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
            .run_node_query(&crate::search::parse_query("type:animal"))
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
