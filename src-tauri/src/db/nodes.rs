use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

impl DbBridge {
    pub fn upsert_node(&self, node: &crate::models::node::NodeMetadata) -> AppResult<()> {
        let properties_json = serde_json::to_string(&node.properties)?;
        self.conn.execute(
            "INSERT INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp, stable_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                node_type=excluded.node_type,
                title=excluded.title,
                content=excluded.content,
                properties=excluded.properties,
                updated_at=excluded.updated_at,
                timestamp=excluded.timestamp,
                stable_id=excluded.stable_id",
            params![node.id, node.node_type, node.title, node.content, properties_json, node.created_at, node.updated_at, node.timestamp, node.stable_id()],
        ).map_err(|e| AppError::General(format!("DB Upsert Node Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM nodes WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Node Error: {}", e)))?;
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE id = ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let mut rows = stmt
            .query_map(params![id], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    pub fn get_nodes_by_type(
        &self,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map(params![node_type], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    /// How many nodes of a type exist. Counted in SQLite, so the answer
    /// costs the same whether the vault holds ten caps or ten thousand.
    pub fn count_nodes_by_type(&self, node_type: &str) -> AppResult<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE node_type = ?1",
                params![node_type],
                |row| row.get(0),
            )
            .map_err(|e| AppError::General(format!("DB Count Error: {}", e)))
    }

    /// Caps still asking to be dealt with: everything except the archived.
    ///
    /// The badge counts this rather than every cap. Archiving is the user
    /// saying "keep it, stop asking" — a number that kept counting those would
    /// be a number they learn to ignore, which is the one thing a pressure
    /// signal must not become.
    pub fn count_inbox_caps(&self) -> AppResult<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM nodes
                 WHERE node_type = 'quickcap'
                   AND COALESCE(json_extract(properties, '$.archived'), 0) NOT IN (1, 'true')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| AppError::General(format!("DB Count Error: {}", e)))
    }

    /// How much of a node's body a list screen gets to see.
    const PREVIEW_CHARS: i64 = 150;

    /// Every node of a type, without their bodies.
    ///
    /// `substr` runs inside SQLite, so a five-thousand-note vault moves a
    /// couple of hundred kilobytes here instead of twenty megabytes. SQLite
    /// counts characters rather than bytes on a TEXT column, so the cut cannot
    /// land in the middle of one.
    pub fn get_node_summaries_by_type(
        &self,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, node_type, title, substr(content, 1, ?2), properties,
                        created_at, updated_at, timestamp
                 FROM nodes WHERE node_type = ?1 ORDER BY updated_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (summaries): {}", e)))?;

        let rows = stmt
            .query_map(params![node_type, Self::PREVIEW_CHARS], |row| {
                let props_str: String = row.get(4)?;
                Ok(crate::models::node::NodeSummary {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    preview: row.get(3)?,
                    properties: serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (summaries): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// Find the `file` node describing a given path on disk.
    ///
    /// File nodes are keyed by UUID rather than by path, so this is how the
    /// scanner recognises a file it has already seen. It used to be answered by
    /// walking every file node in the database on each file examined, which
    /// made indexing a folder quadratic in the number of files in it. The
    /// expression index declared alongside the `nodes` table is what lets the
    /// same question be asked directly.
    pub fn get_file_node_by_path(
        &self,
        path: &str,
    ) -> AppResult<Option<crate::models::node::NodeMetadata>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp
                 FROM nodes
                 WHERE node_type = 'file' AND json_extract(properties, '$.path') = ?1",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (file by path): {}", e)))?;

        let mut rows = stmt
            .query_map(params![path], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (file by path): {}", e)))?;

        Ok(rows.next().and_then(|r| r.ok()))
    }

    /// The latest day each person was involved in anything.
    ///
    /// This is what answers "when did I last speak to them" without anybody
    /// having to remember to write it down. A note that mentions somebody, a
    /// task about them, a meeting they were in — each one is a touch, and the
    /// links are already recorded in `node_edges`.
    ///
    /// Deriving it beats storing it. Writing a `last_contacted` into a
    /// person's file every time a note mentions them would mean a write, a
    /// CRDT commit and a sync round for every note saved, to record something
    /// the database can already work out.
    ///
    /// Keyed by the person's vault path, which is what the rest of the app
    /// calls a node. Only the kinds of node that mean contact are counted: a
    /// file attached to somebody is not a conversation.
    pub fn last_contact_by_person(&self) -> AppResult<std::collections::HashMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT p.id, MAX(substr(n.updated_at, 1, 10))
                 FROM node_edges e
                 JOIN nodes n ON n.stable_id = e.source_id
                 JOIN nodes p ON p.stable_id = e.target_id
                 WHERE p.node_type = 'person'
                   AND n.node_type IN ('note', 'task', 'quickcap', 'event')
                 GROUP BY p.id",
            )
            .map_err(|e| {
                AppError::General(format!("DB Query Error (last_contact_by_person): {}", e))
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (last_contact): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// Everything that could produce a reminder in the next few days.
    ///
    /// People are in here for two reasons — a birthday, and a cadence they
    /// asked to be held to — and their `last_contacted` is topped up from
    /// [`Self::last_contact_by_person`] on the way out, so the planner sees
    /// the later of "what they logged" and "what the vault knows".
    ///
    /// The debts ledger is the odd one: it is a single node holding a list, so
    /// it is fetched whole and `reminders::plan_debts` walks it. The filter
    /// only asks whether there is anything in the list at all, because whether
    /// a particular debt has a due date is a question about an array element
    /// and belongs with the planner rather than in SQL.
    pub fn get_active_tasks_and_events(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE node_type IN ('task', 'event', 'person', 'finance_debts') 
             AND (
                 (node_type = 'task' AND json_extract(properties, '$.status') NOT IN ('done', 'canceled') AND json_extract(properties, '$.due_date') IS NOT NULL AND json_extract(properties, '$.due_date') != '')
                 OR (node_type = 'event' AND json_extract(properties, '$.start_at') IS NOT NULL AND json_extract(properties, '$.start_at') != '')
                 OR (node_type = 'person' AND (
                        (json_extract(properties, '$.birthday') IS NOT NULL AND json_extract(properties, '$.birthday') != '')
                     OR (json_extract(properties, '$.contact_frequency') IS NOT NULL AND json_extract(properties, '$.contact_frequency') != '')
                 ))
                 OR (node_type = 'finance_debts' AND json_array_length(json_extract(properties, '$.debts')) > 0)
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_active_tasks_and_events): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| {
                AppError::General(format!("DB Map Error (get_active_tasks_and_events): {}", e))
            })?;

        let mut results: Vec<crate::models::node::NodeMetadata> = rows.flatten().collect();

        // Top up each person's last contact from what the vault knows. The
        // later of the two wins: somebody who logged a coffee on Tuesday and
        // wrote a note about the same person on Thursday was in touch on
        // Thursday, and the cadence should count from then.
        let derived = self.last_contact_by_person().unwrap_or_default();
        for node in results.iter_mut().filter(|n| n.node_type == "person") {
            let Some(seen) = derived.get(&node.id) else { continue };
            let stored = node
                .properties
                .get("last_contacted")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Both are `YYYY-MM-DD`, so comparing them as text compares them
            // as dates.
            if seen.as_str() > stored {
                if let Some(map) = node.properties.as_object_mut() {
                    map.insert(
                        "last_contacted".to_string(),
                        serde_json::Value::String(seen.clone()),
                    );
                }
            }
        }

        Ok(results)
    }

    /// Every event in the vault, as calendar summaries — no bodies.
    ///
    /// All of them, not a date range: a weekly series that began in 2020 still
    /// lands on days in 2026, so `start_at` cannot be used to filter rows
    /// without dropping exactly the events a calendar most needs. The range is
    /// applied in `calendar::recurrence::expand_range`, which knows how to ask
    /// the question properly.
    pub fn get_event_summaries(
        &self,
    ) -> AppResult<Vec<crate::calendar::recurrence::EventSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, properties, created_at
                 FROM nodes WHERE node_type = 'event'",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (events): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let props_str: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::calendar::recurrence::EventSummary::from_properties(
                    &id,
                    &title,
                    &created_at,
                    &properties,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (events): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// The tasks that land on a day between `from` and `to`.
    ///
    /// The other half of what was done for events. A calendar used to hold
    /// every task in the vault and run a linear filter for each day cell —
    /// forty-two of them for a month, three hundred and sixty-five for a
    /// year. The days on screen are what it needs, so the days on screen are
    /// what it asks for.
    pub fn get_tasks_in_range(
        &self,
        from: &str,
        to: &str,
    ) -> AppResult<Vec<crate::models::node::NodeSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, node_type, title, '', properties, created_at, updated_at, timestamp
                 FROM nodes
                 WHERE node_type = 'task'
                   AND (
                       (json_extract(properties, '$.due_date')   BETWEEN ?1 AND ?2)
                       OR (json_extract(properties, '$.start_date') BETWEEN ?1 AND ?2)
                   )",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (tasks in range): {}", e)))?;

        let rows = stmt
            .query_map(params![from, to], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeSummary {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    preview: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (tasks in range): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// Every event that names `person_id`, as calendar summaries.
    ///
    /// The question this exists for is the one no ordinary calendar answers:
    /// "every meeting with Anh". Edges are recorded between stable identities
    /// rather than paths, so a person who has been renamed still matches the
    /// events that named them.
    pub fn events_linked_to(
        &self,
        person_id: &str,
    ) -> AppResult<Vec<crate::calendar::recurrence::EventSummary>> {
        if person_id.trim().is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(
                "SELECT n.id, n.title, n.properties, n.created_at
                 FROM node_edges e
                 JOIN nodes n ON n.stable_id = e.source_id
                 WHERE n.node_type = 'event'
                   AND e.target_id = COALESCE(
                       (SELECT stable_id FROM nodes WHERE id = ?1),
                       ?1
                   )",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (linked events): {}", e)))?;

        let rows = stmt
            .query_map(params![person_id], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let props_str: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::calendar::recurrence::EventSummary::from_properties(
                    &id,
                    &title,
                    &created_at,
                    &properties,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (linked events): {}", e)))?;
        Ok(rows.flatten().collect())
    }

    /// The events whose id is in `ids`, as calendar summaries.
    ///
    /// What a text search hands back is a set of ids; turning those into
    /// something the calendar can draw is this.
    pub fn event_summaries_by_id(
        &self,
        ids: &[String],
    ) -> AppResult<Vec<crate::calendar::recurrence::EventSummary>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
        Ok(self
            .get_event_summaries()?
            .into_iter()
            .filter(|e| wanted.contains(e.id.as_str()))
            .collect())
    }

    /// A recurring event and every node split off from it.
    ///
    /// Editing or deleting a whole series has to see all of its parts, and the
    /// parts can sit outside whatever range the calendar is showing — which is
    /// why this is its own query rather than a filter over what is on screen.
    pub fn get_event_series(
        &self,
        root_id: &str,
    ) -> AppResult<Vec<crate::calendar::recurrence::EventSummary>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, properties, created_at
                 FROM nodes
                 WHERE node_type = 'event'
                   AND (id = ?1 OR json_extract(properties, '$.series_id') = ?1)",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (series): {}", e)))?;

        let rows = stmt
            .query_map(params![root_id], |row| {
                let id: String = row.get(0)?;
                let title: String = row.get(1)?;
                let props_str: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::calendar::recurrence::EventSummary::from_properties(
                    &id,
                    &title,
                    &created_at,
                    &properties,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (series): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn get_linked_nodes(
        &self,
        _target_title: &str,
        target_id: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        if target_id.is_empty() {
            return Ok(Vec::new());
        }

        // Edges are recorded between stable identities rather than paths, so a
        // caller naming a node by its current path has to be translated first.
        // Callers that already hold a stable id — the assistant's tools, an
        // edge's own `target_id` — pass through the COALESCE untouched.
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.node_type, n.title, n.content, n.properties, n.created_at, n.updated_at, n.timestamp
             FROM node_edges e
             JOIN nodes n ON n.stable_id = e.source_id
             WHERE e.target_id = COALESCE(
                 (SELECT stable_id FROM nodes WHERE id = ?1),
                 ?1
             )
             ORDER BY n.updated_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_linked_nodes): {}", e)))?;

        let rows = stmt
            .query_map(params![target_id], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (get_linked_nodes): {}", e)))?;

        let mut results = Vec::new();
        for node in rows.flatten() {
            results.push(node);
        }
        Ok(results)
    }

    pub fn get_node_title(&self, node_id: &str) -> Option<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT title FROM nodes WHERE id = ?1")
            .ok()?;
        stmt.query_row(params![node_id], |row| row.get::<_, String>(0))
            .ok()
    }

    pub fn get_all_nodes(&self) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare("SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp FROM nodes ORDER BY updated_at DESC")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }

    /// What types exist in this vault, and which frontmatter keys each one uses.
    ///
    /// The vault describing itself. There is no schema anywhere to read — a
    /// type is whatever somebody wrote in a `type:` field — so the only honest
    /// answer is what is actually there: every distinct `node_type`, how many
    /// nodes carry it, and the union of the frontmatter keys those nodes have.
    ///
    /// Two things this is deliberately not. It is not a list of *permitted*
    /// keys: a node missing one is normal and a node with an extra one is the
    /// user inventing a field, which is the behaviour `NodeType::Other` exists
    /// to protect. And it is not exhaustive per node — a key that appears on
    /// one node of a type appears in that type's list.
    ///
    /// `key_limit` caps the keys reported per type so that one node with a
    /// large generated blob cannot flood the answer.
    pub fn observed_schemas(&self, key_limit: usize) -> AppResult<Vec<(String, i64, Vec<String>)>> {
        let mut counts = self
            .conn
            .prepare(
                "SELECT node_type, COUNT(*) FROM nodes
                 GROUP BY node_type ORDER BY COUNT(*) DESC, node_type ASC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (observed_schemas): {}", e)))?;

        let types: Vec<(String, i64)> = counts
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?
            .flatten()
            .collect();

        // Keys per type in one pass rather than a query each: a vault with
        // thirty invented types would otherwise be thirty round trips.
        let mut keys = self
            .conn
            .prepare(
                "SELECT node_type, json_each.key, COUNT(*) AS n
                 FROM nodes, json_each(nodes.properties)
                 WHERE json_valid(nodes.properties)
                 GROUP BY node_type, json_each.key
                 ORDER BY node_type ASC, n DESC, json_each.key ASC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (observed_keys): {}", e)))?;

        let mut by_type: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for (node_type, key) in keys
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?
            .flatten()
        {
            let entry = by_type.entry(node_type).or_default();
            if entry.len() < key_limit {
                entry.push(key);
            }
        }

        Ok(types
            .into_iter()
            .map(|(node_type, count)| {
                let keys = by_type.remove(&node_type).unwrap_or_default();
                (node_type, count, keys)
            })
            .collect())
    }

    pub fn get_all_tags_with_counts(&self) -> AppResult<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT json_each.value, COUNT(*) 
             FROM nodes, json_each(nodes.properties, '$.tags') 
             GROUP BY json_each.value 
             ORDER BY COUNT(*) DESC, json_each.value ASC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (get_all_tags): {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let tag: String = row.get(0)?;
                let count: i64 = row.get(1)?;
                Ok((tag, count))
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut results = Vec::new();
        for row in rows.flatten() {
            results.push(row);
        }
        Ok(results)
    }

    pub fn get_nodes_by_tag(
        &self,
        target_tag: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, node_type, title, content, properties, created_at, updated_at, timestamp 
             FROM nodes 
             WHERE EXISTS (
                 SELECT 1 FROM json_each(nodes.properties, '$.tags') WHERE value = ?1
             )"
        ).map_err(|e| AppError::General(format!("DB Query Error (get_nodes_by_tag): {}", e)))?;

        let rows = stmt
            .query_map(params![target_tag], |row| {
                let props_str: String = row.get(4)?;
                let properties: serde_json::Value =
                    serde_json::from_str(&props_str).unwrap_or(serde_json::Value::Null);
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut nodes = Vec::new();
        for n in rows.flatten() {
            nodes.push(n);
        }
        Ok(nodes)
    }
}

/// Tests for the query layer over the `nodes` table.
///
/// The Universal Node model keeps every type in one table and pushes each
/// type's own fields into a JSON `properties` blob. That makes the schema
/// trivial and the queries subtle: filters like "an unfinished task that has a
/// due date" are `json_extract` expressions, invisible to the type system and
/// unchecked by the compiler. These tests are the only thing standing between
/// a typo in one of those paths and a list that silently comes back empty.
#[cfg(test)]
mod tests {
    use crate::db::DbBridge;
    use crate::models::node::NodeMetadata;
    use serde_json::json;

    fn node(id: &str, node_type: &str, properties: serde_json::Value) -> NodeMetadata {
        NodeMetadata {
            id: id.to_string(),
            node_type: node_type.to_string(),
            title: id.to_string(),
            content: String::new(),
            properties,
            created_at: "2026-01-01 00:00:00".to_string(),
            updated_at: "2026-01-01 00:00:00".to_string(),
            timestamp: 0,
            blocks: None,
        }
    }

    /// `updated_at` is a TEXT column sorted as text, so callers only get a
    /// meaningful order out of it while the format stays fixed-width.
    fn node_updated(id: &str, node_type: &str, updated_at: &str) -> NodeMetadata {
        let mut n = node(id, node_type, json!({}));
        n.updated_at = updated_at.to_string();
        n
    }

    fn db() -> DbBridge {
        DbBridge::new_in_memory_full().expect("full in-memory schema")
    }

    /// A node with a stable identity of its own, which is what edges name.
    fn node_with_identity(id: &str, node_type: &str, stable: &str, updated_at: &str) -> NodeMetadata {
        let mut n = node(id, node_type, json!({ "node_id": stable }));
        n.updated_at = updated_at.to_string();
        n
    }

    fn edge(source: &str, target: &str) -> crate::db::edges::NodeEdge {
        crate::db::edges::NodeEdge {
            id: format!("{}->{}", source, target),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        }
    }

    /// The world model an assistant reads before it knows anything.
    ///
    /// The point is the invented type: nobody wrote a line of code for
    /// `book`, and it still has to appear here with the fields the user
    /// actually gave it. That is what lets a tool set with no `book` in it
    /// find, read and create books.
    #[test]
    fn a_type_nobody_coded_for_still_describes_itself() {
        let db = db();
        db.upsert_node(&node("Notes/a.md", "note", json!({ "tags": ["x"] })))
            .expect("insert");
        db.upsert_node(&node(
            "Books/dune.md",
            "book",
            json!({ "author": "Herbert", "rating": 5 }),
        ))
        .expect("insert");
        db.upsert_node(&node(
            "Books/ubik.md",
            "book",
            json!({ "author": "Dick", "status": "reading" }),
        ))
        .expect("insert");

        let schemas = db.observed_schemas(25).expect("schemas");

        // Ordered by how much of the vault each type is, so the most useful
        // answer comes first when the list is long.
        let books = schemas
            .iter()
            .find(|(t, ..)| t == "book")
            .expect("book is described even though no code mentions it");
        assert_eq!(books.1, 2);

        // The union across nodes of the type, not the intersection: `rating`
        // is on one book and `status` on the other, and both are real fields
        // of this vault's books.
        let mut fields = books.2.clone();
        fields.sort();
        assert_eq!(fields, vec!["author", "rating", "status"]);

        let notes = schemas.iter().find(|(t, ..)| t == "note").expect("note");
        assert_eq!(notes.1, 1);
        assert_eq!(notes.2, vec!["tags"]);
    }

    /// One node carrying a large generated blob must not crowd every other
    /// type out of the answer.
    #[test]
    fn the_field_list_is_capped_per_type() {
        let db = db();
        let mut wide = serde_json::Map::new();
        for i in 0..40 {
            wide.insert(format!("k{i:02}"), json!(i));
        }
        db.upsert_node(&node("Notes/wide.md", "note", json!(wide)))
            .expect("insert");

        let schemas = db.observed_schemas(5).expect("schemas");
        let notes = schemas.iter().find(|(t, ..)| t == "note").expect("note");
        assert_eq!(notes.2.len(), 5);
    }

    /// Frontmatter that will not parse is a file the user is mid-edit on, not
    /// a reason to fail the whole description.
    #[test]
    fn unreadable_properties_do_not_sink_the_answer() {
        let db = db();
        db.upsert_node(&node("Notes/ok.md", "note", json!({ "tags": [] })))
            .expect("insert");
        db.conn
            .execute(
                "INSERT INTO nodes (id, node_type, title, content, properties, created_at, updated_at, timestamp, stable_id)
                 VALUES ('Notes/bad.md', 'note', 'bad', '', 'not json', '', '', 0, 'bad')",
                [],
            )
            .expect("insert raw");

        let schemas = db.observed_schemas(25).expect("schemas");
        let notes = schemas.iter().find(|(t, ..)| t == "note").expect("note");
        assert_eq!(notes.1, 2, "both nodes are counted");
        assert_eq!(notes.2, vec!["tags"], "only the readable one contributes fields");
    }

    #[test]
    fn a_node_survives_a_round_trip_with_its_properties_intact() {
        let db = db();
        db.upsert_node(&node(
            "Tasks/a.md",
            "task",
            json!({"status": "doing", "n": 3}),
        ))
        .unwrap();

        let found = db
            .get_node("Tasks/a.md")
            .unwrap()
            .expect("node should exist");

        assert_eq!(found.node_type, "task");
        assert_eq!(
            found.properties.get("status").and_then(|v| v.as_str()),
            Some("doing")
        );
        assert_eq!(found.properties.get("n").and_then(|v| v.as_i64()), Some(3));
        assert!(db.get_node("Tasks/missing.md").unwrap().is_none());
    }

    /// The calendar reads events through this query, so the frontmatter shapes
    /// it has to understand are pinned against a real database rather than a
    /// hand-built `serde_json::Value`.
    #[test]
    fn events_come_back_as_calendar_summaries_without_their_bodies() {
        let db = db();
        let mut ev = node(
            "Events/standup.md",
            "event",
            json!({
                "start_at": "2026-03-02T09:00",
                "end_at": "2026-03-02T09:15",
                "recurrence": "weekly",
                "location": "Zoom",
                "tags": ["work"],
            }),
        );
        ev.content = "a body nobody asked for".to_string();
        db.upsert_node(&ev).unwrap();
        db.upsert_node(&node("Notes/n.md", "note", json!({}))).unwrap();

        let found = db.get_event_summaries().unwrap();
        assert_eq!(found.len(), 1, "only events, and only once");
        assert_eq!(found[0].id, "Events/standup.md");
        assert_eq!(found[0].recurrence, "weekly");
        assert_eq!(found[0].location, "Zoom");
        assert_eq!(found[0].tags, vec!["work".to_string()]);
        // There is no field to carry the body, which is the point: a range
        // query used to send every event's text with it.
        assert!(found[0].occurs_on("2026-03-09"));
    }

    /// The reminder loop reads events through this same query. It used to read
    /// `recurrence` out of the properties itself, which meant an event stored
    /// with an `rrule` looked like it never repeated and was reminded once.
    #[test]
    fn an_event_stored_with_a_rule_repeats_for_everyone_who_reads_it() {
        let db = db();
        db.upsert_node(&node(
            "Events/standup.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00", "rrule": "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE" }),
        ))
        .unwrap();

        let found = db.get_event_summaries().unwrap();
        assert_eq!(found[0].rrule, "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE");
        assert!(found[0].occurs_on("2026-03-02"));
        assert!(found[0].occurs_on("2026-03-04"));
        assert!(!found[0].occurs_on("2026-03-09"), "the odd week is skipped");
        assert!(found[0].occurs_on("2026-03-16"));
    }

    /// A rule wins outright over the keys an older version left in the file.
    /// Reading both and merging them is how two sources of truth are made.
    #[test]
    fn a_stored_rule_is_not_second_guessed_by_the_old_keys() {
        let db = db();
        db.upsert_node(&node(
            "Events/mixed.md",
            "event",
            json!({
                "start_at": "2026-03-02",
                "recurrence": "daily",
                "recurrence_end_at": "2026-03-04",
                "rrule": "FREQ=WEEKLY"
            }),
        ))
        .unwrap();

        let found = db.get_event_summaries().unwrap();
        assert!(!found[0].occurs_on("2026-03-03"), "the daily key must be ignored");
        assert!(found[0].occurs_on("2026-03-09"), "and the end date with it");
    }

    /// A vault written before `start_at` existed. If this query stopped
    /// applying the fallback, those events would simply vanish from the
    /// calendar with nothing to show the user why.
    #[test]
    fn an_event_from_an_older_vault_still_comes_back() {
        let db = db();
        db.upsert_node(&node(
            "Events/old.md",
            "event",
            json!({ "event_date": "2026-03-02", "start_time": "09:00" }),
        ))
        .unwrap();

        let found = db.get_event_summaries().unwrap();
        assert_eq!(found[0].start_at, "2026-03-02T09:00:00");
        assert!(found[0].occurs_on("2026-03-02"));
    }

    /// Editing "all events in the series" has to reach parts that fall outside
    /// the days on screen, which is what this query exists for.
    #[test]
    fn a_series_is_found_by_its_root_wherever_its_parts_fall() {
        let db = db();
        db.upsert_node(&node(
            "Events/root.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00", "recurrence": "weekly" }),
        ))
        .unwrap();
        db.upsert_node(&node(
            "Events/split.md",
            "event",
            json!({ "start_at": "2027-11-30T11:00", "series_id": "Events/root.md" }),
        ))
        .unwrap();
        db.upsert_node(&node(
            "Events/unrelated.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00" }),
        ))
        .unwrap();

        let mut ids: Vec<String> = db
            .get_event_series("Events/root.md")
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["Events/root.md", "Events/split.md"]);
    }

    #[test]
    fn asking_for_a_series_that_is_not_one_returns_just_that_event() {
        let db = db();
        db.upsert_node(&node(
            "Events/solo.md",
            "event",
            json!({ "start_at": "2026-03-02T09:00" }),
        ))
        .unwrap();
        assert_eq!(db.get_event_series("Events/solo.md").unwrap().len(), 1);
        assert!(db.get_event_series("Events/nope.md").unwrap().is_empty());
    }

    /// The id is the primary key, so writing the same path twice must update
    /// rather than accumulate. The vault scan re-upserts on every edit.
    #[test]
    fn upserting_the_same_id_updates_in_place_instead_of_duplicating() {
        let db = db();
        db.upsert_node(&node("n.md", "note", json!({"v": 1})))
            .unwrap();
        db.upsert_node(&node("n.md", "task", json!({"v": 2})))
            .unwrap();

        let all = db.get_all_nodes().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].node_type, "task");
        assert_eq!(all[0].properties.get("v").and_then(|v| v.as_i64()), Some(2));
    }

    #[test]
    fn nodes_are_filtered_by_type_and_returned_newest_first() {
        let db = db();
        db.upsert_node(&node_updated("old.md", "note", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("new.md", "note", "2026-06-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("t.md", "task", "2026-09-01 00:00:00"))
            .unwrap();

        let notes = db.get_nodes_by_type("note").unwrap();

        assert_eq!(
            notes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["new.md", "old.md"]
        );
    }

    #[test]
    fn deleting_a_node_removes_it() {
        let db = db();
        db.upsert_node(&node("n.md", "note", json!({}))).unwrap();
        db.delete_node("n.md").unwrap();

        assert!(db.get_node("n.md").unwrap().is_none());
        // Deleting something already gone is not an error — the vault scan
        // relies on being able to fire it blindly.
        db.delete_node("n.md").unwrap();
    }

    #[test]
    fn a_node_is_found_by_any_one_of_its_tags() {
        let db = db();
        db.upsert_node(&node("a.md", "note", json!({"tags": ["work", "urgent"]})))
            .unwrap();
        db.upsert_node(&node("b.md", "note", json!({"tags": ["home"]})))
            .unwrap();
        db.upsert_node(&node("c.md", "note", json!({}))).unwrap();

        assert_eq!(db.get_nodes_by_tag("work").unwrap().len(), 1);
        assert_eq!(db.get_nodes_by_tag("urgent").unwrap()[0].id, "a.md");
        assert_eq!(db.get_nodes_by_tag("home").unwrap()[0].id, "b.md");
        assert!(db.get_nodes_by_tag("nonexistent").unwrap().is_empty());
    }

    /// Tag counts drive the tag sidebar. A node carrying no tags at all must
    /// not break the aggregate — untagged notes are the common case.
    #[test]
    fn tags_are_counted_across_nodes_most_used_first() {
        let db = db();
        db.upsert_node(&node("a.md", "note", json!({"tags": ["work", "urgent"]})))
            .unwrap();
        db.upsert_node(&node("b.md", "task", json!({"tags": ["work"]})))
            .unwrap();
        db.upsert_node(&node("c.md", "note", json!({}))).unwrap();

        let counts = db.get_all_tags_with_counts().unwrap();

        assert_eq!(counts[0], ("work".to_string(), 2));
        assert_eq!(counts[1], ("urgent".to_string(), 1));
        assert_eq!(counts.len(), 2);
    }

    /// The agenda query, and the densest `json_extract` expression in the
    /// codebase. Each exclusion below is a separate clause in that SQL; if any
    /// one of the JSON paths is wrong, the agenda quietly loses a whole class
    /// of item rather than failing.
    #[test]
    fn the_agenda_takes_only_dated_open_items() {
        let db = db();

        db.upsert_node(&node(
            "open.md",
            "task",
            json!({"status": "doing", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "done.md",
            "task",
            json!({"status": "done", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "canceled.md",
            "task",
            json!({"status": "canceled", "due_date": "2026-08-20"}),
        ))
        .unwrap();
        db.upsert_node(&node("undated.md", "task", json!({"status": "doing"})))
            .unwrap();
        db.upsert_node(&node(
            "blank-date.md",
            "task",
            json!({"status": "doing", "due_date": ""}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "ev.md",
            "event",
            json!({"start_at": "2026-08-20T09:00"}),
        ))
        .unwrap();
        db.upsert_node(&node("ev-undated.md", "event", json!({})))
            .unwrap();
        db.upsert_node(&node("p.md", "person", json!({"birthday": "1990-05-05"})))
            .unwrap();
        db.upsert_node(&node("p-nobday.md", "person", json!({})))
            .unwrap();
        db.upsert_node(&node(
            "note.md",
            "note",
            json!({"due_date": "2026-08-20", "status": "doing"}),
        ))
        .unwrap();

        let mut ids = db
            .get_active_tasks_and_events()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect::<Vec<_>>();
        ids.sort();

        assert_eq!(ids, vec!["ev.md", "open.md", "p.md"]);
    }

    /// Backlinks resolve through `node_edges` by id alone. The title argument
    /// the caller passes is ignored, which matters: a node is reachable from
    /// its backlinks even after it has been renamed.
    #[test]
    fn backlinks_are_resolved_by_id_and_ignore_the_title_argument() {
        use crate::db::NodeEdge;

        let db = db();
        db.upsert_node(&node("src.md", "note", json!({}))).unwrap();
        db.upsert_node(&node("target.md", "project", json!({})))
            .unwrap();
        db.upsert_node(&node("unrelated.md", "note", json!({})))
            .unwrap();
        db.upsert_node_edge(&NodeEdge {
            id: "e1".to_string(),
            source_id: "src.md".to_string(),
            target_id: "target.md".to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        })
        .unwrap();

        let linked = db
            .get_linked_nodes("a title nobody has", "target.md")
            .unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, "src.md");

        // An empty id must not be read as "match everything".
        assert!(db.get_linked_nodes("target", "").unwrap().is_empty());
    }

    /// The point of the summary query: the body does not come along.
    #[test]
    fn a_summary_carries_the_opening_of_the_body_and_not_the_body() {
        let db = db();
        let long_body = "A".repeat(5_000);
        let mut n = node(
            "Notes/a.md",
            "note",
            json!({"tags": ["work"], "pinned": true}),
        );
        n.title = "A note".to_string();
        n.content = long_body;
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].title, "A note");
        assert_eq!(summaries[0].preview.chars().count(), 150);
        assert_eq!(
            summaries[0]
                .properties
                .get("pinned")
                .and_then(|v| v.as_bool()),
            Some(true),
            "properties drive the list, so they must survive"
        );
    }

    /// The cut happens in SQL, where a naive byte offset would slice a
    /// multi-byte character in half and hand the frontend broken text.
    #[test]
    fn a_preview_never_cuts_a_character_in_half() {
        let db = db();
        let mut n = node("Notes/vi.md", "note", json!({}));
        // Vietnamese, so every character is multi-byte in UTF-8.
        n.content = "Đường đi khó, không khó vì ngăn sông cách núi. ".repeat(20);
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        let preview = &summaries[0].preview;

        assert_eq!(preview.chars().count(), 150);
        assert!(
            n.content.starts_with(preview.as_str()),
            "the preview should be the opening of the body, intact"
        );
    }

    #[test]
    fn a_body_shorter_than_the_preview_comes_back_whole() {
        let db = db();
        let mut n = node("Notes/short.md", "note", json!({}));
        n.content = "Just a line.".to_string();
        db.upsert_node(&n).unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        assert_eq!(summaries[0].preview, "Just a line.");
    }

    #[test]
    fn summaries_are_filtered_by_type_and_ordered_like_the_full_query() {
        let db = db();
        db.upsert_node(&node_updated("old.md", "note", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("new.md", "note", "2026-06-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_updated("t.md", "task", "2026-09-01 00:00:00"))
            .unwrap();

        let summaries = db.get_node_summaries_by_type("note").unwrap();
        assert_eq!(
            summaries.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            vec!["new.md", "old.md"]
        );
    }

    /// The scanner's "have I seen this file before?" question. It has to match
    /// on the path inside the node's properties, because a file node's id is a
    /// UUID that says nothing about where the file is.
    #[test]
    fn a_file_node_is_found_by_the_path_it_describes() {
        let db = db();
        db.upsert_node(&node(
            "uuid-1",
            "file",
            json!({"path": "/Users/x/Documents/report.pdf"}),
        ))
        .unwrap();
        db.upsert_node(&node(
            "uuid-2",
            "file",
            json!({"path": "/Users/x/other.pdf"}),
        ))
        .unwrap();

        let found = db
            .get_file_node_by_path("/Users/x/Documents/report.pdf")
            .unwrap()
            .expect("the file node should be found by its path");
        assert_eq!(found.id, "uuid-1");

        assert!(db
            .get_file_node_by_path("/Users/x/never-seen.pdf")
            .unwrap()
            .is_none());
    }

    /// The lookup is restricted to file nodes. Disk-backed nodes also carry a
    /// `path` property in places, and matching one of those would hand the
    /// scanner someone else's node to overwrite.
    #[test]
    fn the_file_lookup_ignores_nodes_of_other_types_sharing_a_path() {
        let db = db();
        db.upsert_node(&node(
            "Notes/a.md",
            "note",
            json!({"path": "/Users/x/report.pdf"}),
        ))
        .unwrap();

        assert!(db
            .get_file_node_by_path("/Users/x/report.pdf")
            .unwrap()
            .is_none());
    }

    /// The index behind the lookup is a partial index on a JSON expression;
    /// if it stops being used the query still returns the right answer, so
    /// only the plan reveals a regression.
    #[test]
    fn the_file_path_lookup_uses_its_index_rather_than_reading_every_node() {
        let db = db();
        let plan: String = db
            .conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM nodes \
                 WHERE node_type = 'file' AND json_extract(properties, '$.path') = ?1",
                ["x"],
                |r| r.get(3),
            )
            .unwrap();

        assert!(
            plan.contains("idx_nodes_file_path"),
            "the file path lookup stopped using its index: {plan}"
        );
    }

    /// A node that carries the identity sync gave it.
    fn node_with_stable_id(id: &str, stable: &str) -> NodeMetadata {
        node(id, "note", json!({ "node_id": stable }))
    }

    fn link(source: &str, target: &str) -> crate::db::NodeEdge {
        crate::db::NodeEdge {
            id: format!("{source}->{target}"),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: "wikilink".to_string(),
            relation: None,
            created_at: "2026-01-01 00:00:00".to_string(),
        }
    }

    /// The property the whole change exists for.
    ///
    /// Archiving a task moves its file into `archived/`. Keyed by path, every
    /// backlink to that task pointed at a location nothing was at any more, and
    /// the links vanished from the interface with no error. Keyed by the
    /// identity in the file's own frontmatter, the move is invisible to them.
    #[test]
    fn a_backlink_survives_the_linked_note_moving_to_another_folder() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        let before = db.get_linked_nodes("", "Tasks/target.md").unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].id, "Notes/writer.md");

        // Archive it: the row moves to the new path, the file keeps its id.
        db.delete_node("Tasks/target.md").unwrap();
        db.upsert_node(&node_with_stable_id(
            "Tasks/archived/target.md",
            "uuid-target",
        ))
        .unwrap();

        let after = db.get_linked_nodes("", "Tasks/archived/target.md").unwrap();
        assert_eq!(
            after.len(),
            1,
            "the backlink was lost when the task was archived"
        );
        assert_eq!(after[0].id, "Notes/writer.md");
    }

    /// The same holds for the note doing the linking.
    #[test]
    fn a_backlink_survives_the_linking_note_moving() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        db.delete_node("Notes/writer.md").unwrap();
        db.upsert_node(&node_with_stable_id("Archive/writer.md", "uuid-writer"))
            .unwrap();

        let linked = db.get_linked_nodes("", "Tasks/target.md").unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(
            linked[0].id, "Archive/writer.md",
            "the backlink should now report the note's new home"
        );
    }

    /// A caller may hold either name for a node. Both have to find its links,
    /// because the assistant's tools pass stable ids while the interface passes
    /// paths.
    #[test]
    fn links_are_found_whether_the_caller_names_the_path_or_the_identity() {
        let db = db();
        db.upsert_node(&node_with_stable_id("Notes/writer.md", "uuid-writer"))
            .unwrap();
        db.upsert_node(&node_with_stable_id("Tasks/target.md", "uuid-target"))
            .unwrap();
        db.upsert_node_edge(&link("uuid-writer", "uuid-target"))
            .unwrap();

        assert_eq!(db.get_linked_nodes("", "Tasks/target.md").unwrap().len(), 1);
        assert_eq!(db.get_linked_nodes("", "uuid-target").unwrap().len(), 1);
    }

    /// Files that have never been given an identity still work, keyed by path
    /// exactly as before. A vault mid-upgrade is full of them.
    #[test]
    fn a_node_without_a_stable_identity_still_resolves_by_its_path() {
        let db = db();
        db.upsert_node(&node("Notes/plain.md", "note", json!({})))
            .unwrap();
        db.upsert_node(&node("Notes/other.md", "note", json!({})))
            .unwrap();
        db.upsert_node_edge(&link("Notes/plain.md", "Notes/other.md"))
            .unwrap();

        let linked = db.get_linked_nodes("", "Notes/other.md").unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, "Notes/plain.md");
    }

    /// The backlink join reads a JSON field, which without its index means
    /// reading every node in the vault for every lookup.
    #[test]
    fn the_backlink_join_uses_the_stable_id_index() {
        let db = db();
        // A join plans as several steps; the one that matters is how the nodes
        // side is reached, so read them all rather than only the first.
        let plan: Vec<String> = {
            let mut stmt = db
                .conn()
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT n.id FROM node_edges e \
                     JOIN nodes n ON n.stable_id = e.source_id \
                     WHERE e.target_id = ?1",
                )
                .unwrap();
            let rows = stmt
                .query_map(["x"], |r| r.get::<_, String>(3))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        assert!(
            plan.iter().any(|step| step.contains("idx_nodes_stable_id")),
            "the backlink join stopped using its index:\n  {}",
            plan.join("\n  ")
        );
    }

    #[test]
    fn a_title_can_be_looked_up_without_loading_the_node() {
        let db = db();
        let mut n = node("n.md", "note", json!({}));
        n.title = "Real Title".to_string();
        db.upsert_node(&n).unwrap();

        assert_eq!(db.get_node_title("n.md"), Some("Real Title".to_string()));
        assert_eq!(db.get_node_title("missing.md"), None);
    }

    // ── Last contact, derived ───────────────────────────────

    #[test]
    fn a_note_that_mentions_somebody_counts_as_having_been_in_touch() {
        // The answer to "how does it know when I last spoke to them" that
        // does not involve reading anybody's email.
        let db = db();
        let mut person = node("People/an.md", "person", json!({ "node_id": "an" }));
        person.updated_at = "2026-01-01 00:00:00".to_string();
        db.upsert_node(&person).unwrap();
        db.upsert_node(&node_with_identity("Notes/coffee.md", "note", "coffee", "2026-08-20 09:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("coffee", "an")).unwrap();

        let seen = db.last_contact_by_person().unwrap();
        assert_eq!(seen.get("People/an.md").map(String::as_str), Some("2026-08-20"));
    }

    #[test]
    fn the_latest_touch_is_the_one_that_counts() {
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("Notes/old.md", "note", "old", "2026-02-01 09:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("Tasks/new.md", "task", "new", "2026-08-20 09:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("old", "an")).unwrap();
        db.upsert_node_edge(&edge("new", "an")).unwrap();

        let seen = db.last_contact_by_person().unwrap();
        assert_eq!(seen.get("People/an.md").map(String::as_str), Some("2026-08-20"));
    }

    #[test]
    fn a_file_attached_to_somebody_is_not_a_conversation() {
        // Nor is another person linked to them. Only the kinds of node that
        // record something happening count.
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("Files/cv.pdf", "file", "cv", "2026-08-20 09:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("People/binh.md", "person", "binh", "2026-08-20 09:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("cv", "an")).unwrap();
        db.upsert_node_edge(&edge("binh", "an")).unwrap();

        assert!(db.last_contact_by_person().unwrap().is_empty());
    }

    #[test]
    fn the_planner_sees_the_later_of_what_was_logged_and_what_the_vault_knows() {
        let db = db();
        let mut person = node(
            "People/an.md",
            "person",
            json!({ "node_id": "an", "contact_frequency": "weekly", "last_contacted": "2026-02-01" }),
        );
        person.updated_at = "2026-01-01 00:00:00".to_string();
        db.upsert_node(&person).unwrap();
        db.upsert_node(&node_with_identity("Notes/coffee.md", "note", "coffee", "2026-08-20 09:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("coffee", "an")).unwrap();

        let planned = db.get_active_tasks_and_events().unwrap();
        let an = planned.iter().find(|n| n.id == "People/an.md").expect("person");
        assert_eq!(an.properties["last_contacted"], json!("2026-08-20"));
    }

    #[test]
    fn a_logged_interaction_later_than_anything_in_the_vault_still_wins() {
        // Somebody logs a phone call today; the last note about them was in
        // February. The cadence counts from today.
        let db = db();
        let mut person = node(
            "People/an.md",
            "person",
            json!({ "node_id": "an", "contact_frequency": "weekly", "last_contacted": "2026-09-01" }),
        );
        person.updated_at = "2026-01-01 00:00:00".to_string();
        db.upsert_node(&person).unwrap();
        db.upsert_node(&node_with_identity("Notes/old.md", "note", "old", "2026-02-01 09:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("old", "an")).unwrap();

        let planned = db.get_active_tasks_and_events().unwrap();
        let an = planned.iter().find(|n| n.id == "People/an.md").expect("person");
        assert_eq!(an.properties["last_contacted"], json!("2026-09-01"));
    }

    #[test]
    fn a_person_being_kept_up_with_reaches_the_planner_without_a_birthday() {
        // The query used to ask only for birthdays, so a cadence could be set
        // and never once produce a reminder.
        let db = db();
        db.upsert_node(&node(
            "People/an.md",
            "person",
            json!({ "contact_frequency": "monthly", "last_contacted": "2026-01-01" }),
        ))
        .unwrap();
        db.upsert_node(&node("People/binh.md", "person", json!({}))).unwrap();

        let ids: Vec<String> = db
            .get_active_tasks_and_events()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, ["People/an.md"]);
    }

    /// The reminder planner only ever sees what this query returns, so a node
    /// type it does not ask for is a feature that works in its own tests and
    /// never once fires in the app.
    #[test]
    fn the_debts_ledger_reaches_the_planner() {
        let db = db();
        db.upsert_node(&node(
            "Finance/Debts.json",
            "finance_debts",
            json!({ "debts": [
                { "id": "d1", "type": "lend", "person": "Mai", "dueDate": "2026-09-01", "status": "active" }
            ]}),
        ))
        .unwrap();

        let ids: Vec<String> = db
            .get_active_tasks_and_events()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert!(ids.contains(&"Finance/Debts.json".to_string()), "{ids:?}");
    }

    /// A vault where nobody has recorded a debt should not be carrying an
    /// empty ledger through the planner every few minutes.
    #[test]
    fn an_empty_debts_ledger_does_not_reach_the_planner() {
        let db = db();
        db.upsert_node(&node("Finance/Debts.json", "finance_debts", json!({ "debts": [] })))
            .unwrap();

        let ids: Vec<String> = db
            .get_active_tasks_and_events()
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert!(ids.is_empty(), "{ids:?}");
    }

    /// Months of the ledger are not reminders and must not be dragged in.
    #[test]
    fn the_rest_of_finance_stays_out_of_the_planner() {
        let db = db();
        db.upsert_node(&node(
            "Finance/2026-08.json",
            "finance_month",
            json!({ "transactions": [{ "id": "tx-1", "amount": 100 }] }),
        ))
        .unwrap();
        db.upsert_node(&node("Finance/Config.json", "finance_config", json!({ "currency": "USD" })))
            .unwrap();

        assert!(db.get_active_tasks_and_events().unwrap().is_empty());
    }

    // ── Clearing links ──────────────────────────────────────

    #[test]
    fn a_link_removed_from_a_note_leaves_the_graph() {
        // What this guards: edges are recorded under a node's stable id but
        // were cleared by its path. For any node that has an identity — every
        // node written since identities landed — the two never matched, so
        // nothing was ever cleared and backlinks only ever grew.
        let db = db();
        db.upsert_node(&node_with_identity("Notes/a.md", "note", "uuid-a", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("uuid-a", "uuid-an")).unwrap();
        assert_eq!(db.get_linked_nodes("An", "People/an.md").unwrap().len(), 1);

        db.delete_node_edges_for_path("Notes/a.md").unwrap();
        assert!(
            db.get_linked_nodes("An", "People/an.md").unwrap().is_empty(),
            "the link should be gone"
        );
    }

    #[test]
    fn an_edge_written_before_identities_existed_is_cleared_too() {
        // Rows from an older vault name the path on both ends. A caller
        // holding a path cannot tell which kind it is looking at, so both go.
        let db = db();
        db.upsert_node(&node("Notes/plain.md", "note", json!({}))).unwrap();
        db.upsert_node(&node("People/an.md", "person", json!({}))).unwrap();
        db.upsert_node_edge(&edge("Notes/plain.md", "People/an.md")).unwrap();

        db.delete_node_edges_for_path("Notes/plain.md").unwrap();
        assert!(db.get_linked_nodes("An", "People/an.md").unwrap().is_empty());
    }

    #[test]
    fn clearing_one_notes_links_leaves_another_notes_alone() {
        let db = db();
        db.upsert_node(&node_with_identity("Notes/a.md", "note", "uuid-a", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("Notes/b.md", "note", "uuid-b", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node_edge(&edge("uuid-a", "uuid-an")).unwrap();
        db.upsert_node_edge(&edge("uuid-b", "uuid-an")).unwrap();

        db.delete_node_edges_for_path("Notes/a.md").unwrap();
        let left: Vec<String> = db
            .get_linked_nodes("An", "People/an.md")
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(left, ["Notes/b.md"]);
    }

    // ── Person-to-person links ──────────────────────────────

    fn person_edge(source: &str, target: &str, relation: &str) -> crate::db::edges::NodeEdge {
        crate::db::edges::NodeEdge {
            id: format!("{}->{}", source, target),
            source_id: source.to_string(),
            target_id: target.to_string(),
            edge_type: "person_link".to_string(),
            relation: Some(relation.to_string()),
            created_at: "2026-01-01 00:00:00".to_string(),
        }
    }

    #[test]
    fn a_connection_is_answered_with_who_that_person_is_now() {
        // The point of reading these from the index: the name comes from the
        // other person's own row. Their frontmatter used to carry a copy, and
        // renaming somebody left the old name in everybody else's graph.
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("People/binh.md", "person", "uuid-binh", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node_edge(&person_edge("uuid-an", "uuid-binh", "friend")).unwrap();

        let got = db.person_connections("People/an.md").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].person_id, "People/binh.md");
        assert_eq!(got[0].name, "People/binh.md");
        assert_eq!(got[0].relation_type, "friend");
    }

    #[test]
    fn renaming_somebody_changes_what_everybody_else_calls_them() {
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        let mut binh = node("People/binh.md", "person", json!({ "node_id": "uuid-binh" }));
        binh.title = "Bình".to_string();
        db.upsert_node(&binh).unwrap();
        db.upsert_node_edge(&person_edge("uuid-an", "uuid-binh", "friend")).unwrap();
        assert_eq!(db.person_connections("People/an.md").unwrap()[0].name, "Bình");

        // They change their own name; nobody else's file is touched.
        binh.title = "Trần Bình".to_string();
        db.upsert_node(&binh).unwrap();
        assert_eq!(db.person_connections("People/an.md").unwrap()[0].name, "Trần Bình");
    }

    #[test]
    fn a_link_to_somebody_no_longer_here_is_not_answered_at_all() {
        // An orphan edge cannot become a row in the graph, because the join
        // has nobody to join to. Frontmatter had no such protection: it kept
        // drawing a person who had been deleted, under the cached name.
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node_edge(&person_edge("uuid-an", "uuid-gone", "friend")).unwrap();

        assert!(db.person_connections("People/an.md").unwrap().is_empty());
    }

    #[test]
    fn a_mention_in_a_note_is_not_a_relationship() {
        let db = db();
        db.upsert_node(&node_with_identity("People/an.md", "person", "uuid-an", "2026-01-01 00:00:00"))
            .unwrap();
        db.upsert_node(&node_with_identity("People/binh.md", "person", "uuid-binh", "2026-01-01 00:00:00"))
            .unwrap();
        // An ordinary link, not a person link.
        db.upsert_node_edge(&edge("uuid-an", "uuid-binh")).unwrap();

        assert!(db.person_connections("People/an.md").unwrap().is_empty());
    }

    // ── How you know somebody ───────────────────────────────

    /// A vault of people, linked as named.
    fn social(links: &[(&str, &str)], names: &[&str]) -> DbBridge {
        let db = db();
        for name in names {
            db.upsert_node(&node_with_identity(
                &format!("People/{name}.md"),
                "person",
                &format!("uuid-{name}"),
                "2026-01-01 00:00:00",
            ))
            .unwrap();
        }
        for (a, b) in links {
            db.upsert_node_edge(&person_edge(
                &format!("uuid-{a}"),
                &format!("uuid-{b}"),
                "friend",
            ))
            .unwrap();
        }
        db
    }

    fn path(db: &DbBridge, from: &str, to: &str) -> Vec<String> {
        db.path_between_people(&format!("People/{from}.md"), &format!("People/{to}.md"))
            .unwrap()
            .into_iter()
            .map(|id| id.trim_start_matches("People/").trim_end_matches(".md").to_string())
            .collect()
    }

    #[test]
    fn the_route_between_two_people_is_the_shortest_one() {
        // an → binh → cuong → dung, and also an → em → dung.
        let db = social(
            &[("an", "binh"), ("binh", "cuong"), ("cuong", "dung"), ("an", "em"), ("em", "dung")],
            &["an", "binh", "cuong", "dung", "em"],
        );
        assert_eq!(path(&db, "an", "dung"), ["an", "em", "dung"]);
    }

    #[test]
    fn a_link_is_followed_in_either_direction() {
        // A relationship has no direction the way a citation does: if An is
        // linked to Bình, Bình knows An.
        let db = social(&[("binh", "an")], &["an", "binh"]);
        assert_eq!(path(&db, "an", "binh"), ["an", "binh"]);
    }

    #[test]
    fn somebody_you_are_not_connected_to_has_no_route() {
        let db = social(&[("an", "binh")], &["an", "binh", "stranger"]);
        assert!(path(&db, "an", "stranger").is_empty());
    }

    #[test]
    fn the_route_to_yourself_is_yourself() {
        let db = social(&[], &["an"]);
        assert_eq!(path(&db, "an", "an"), ["an"]);
    }

    #[test]
    fn a_circle_of_friends_does_not_go_round_forever() {
        let db = social(
            &[("an", "binh"), ("binh", "cuong"), ("cuong", "an")],
            &["an", "binh", "cuong", "outsider"],
        );
        assert!(path(&db, "an", "outsider").is_empty(), "it should stop, not loop");
        assert_eq!(path(&db, "an", "cuong"), ["an", "cuong"]);
    }

    #[test]
    fn an_ordinary_mention_is_not_a_way_of_knowing_somebody() {
        // Being named in the same note is not a relationship.
        let db = social(&[], &["an", "binh"]);
        db.upsert_node_edge(&edge("uuid-an", "uuid-binh")).unwrap();
        assert!(path(&db, "an", "binh").is_empty());
    }
}
