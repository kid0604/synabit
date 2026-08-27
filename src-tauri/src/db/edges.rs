use super::DbBridge;
use crate::error::{AppError, AppResult};
use rusqlite::params;

/// One end of a person-to-person link, resolved to who that person is now.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PersonConnection {
    /// The other person's vault path, which is what the app calls a node.
    pub person_id: String,
    /// Their name as it stands today, read from their own row.
    pub name: String,
    pub relation_type: String,
}

/// New ID-based edge for the knowledge graph
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeEdge {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String, // 'wikilink' | 'internal_link' | 'embed' | 'manual'
    pub relation: Option<String>, // 'references' | 'attachment' | 'related' | custom...
    pub created_at: String,
}

impl DbBridge {
    pub fn upsert_node_edge(&self, edge: &NodeEdge) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO node_edges (id, source_id, target_id, edge_type, relation, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(source_id, target_id, edge_type) DO UPDATE SET
                relation = COALESCE(excluded.relation, node_edges.relation),
                id = excluded.id",
                params![
                    edge.id,
                    edge.source_id,
                    edge.target_id,
                    edge.edge_type,
                    edge.relation,
                    edge.created_at
                ],
            )
            .map_err(|e| AppError::General(format!("DB Error upserting node_edge: {}", e)))?;
        Ok(())
    }

    /// Clear every edge out of the node at this path, under either identity.
    ///
    /// Edges are recorded under a node's stable id, but rows written before
    /// identities existed name the path instead, and a caller holding a path
    /// cannot tell which. Both are cleared: the node is going away, and an
    /// edge left behind is a link in the graph to a file that is not there.
    ///
    /// Call this *before* the node's own row is deleted, or the lookup that
    /// finds its stable id has nothing to find.
    /// The shortest chain of links from one person to another.
    ///
    /// "How do I know them" — answered by the graph rather than by memory.
    /// Only worth asking now that a person link means something: until links
    /// were keyed by identity and cleared when removed, a path through them
    /// could run through people who had been deleted or renamed away.
    ///
    /// Breadth-first, so the first route found is the shortest. Links are
    /// followed in both directions: a link is a relationship, and a
    /// relationship does not have a direction the way a citation does.
    ///
    /// Returns the vault paths in order, `from` first and `to` last, or an
    /// empty list when there is no route.
    pub fn path_between_people(&self, from: &str, to: &str) -> AppResult<Vec<String>> {
        if from == to {
            return Ok(vec![from.to_string()]);
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT s.id, t.id
                 FROM node_edges e
                 JOIN nodes s ON s.stable_id = e.source_id
                 JOIN nodes t ON t.stable_id = e.target_id
                 WHERE e.edge_type = 'person_link'
                   AND s.node_type = 'person'
                   AND t.node_type = 'person'",
            )
            .map_err(|e| {
                AppError::General(format!("DB Query Error (path_between_people): {}", e))
            })?;

        let mut neighbours: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (path): {}", e)))?;
        for (a, b) in rows.flatten() {
            neighbours.entry(a.clone()).or_default().push(b.clone());
            neighbours.entry(b).or_default().push(a);
        }

        // Breadth-first from `from`, remembering how each person was reached.
        let mut came_from: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        let mut seen = std::collections::HashSet::new();
        queue.push_back(from.to_string());
        seen.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                let mut path = vec![current.clone()];
                let mut step = current;
                while let Some(previous) = came_from.get(&step) {
                    path.push(previous.clone());
                    step = previous.clone();
                }
                path.reverse();
                return Ok(path);
            }
            for next in neighbours.get(&current).map(Vec::as_slice).unwrap_or(&[]) {
                if seen.insert(next.clone()) {
                    came_from.insert(next.clone(), current.clone());
                    queue.push_back(next.clone());
                }
            }
        }

        Ok(Vec::new())
    }

    /// Everything recorded as being about this person, newest first.
    ///
    /// Interactions live here rather than in an array inside the person's own
    /// file, and that is not a tidiness argument. A `.md` file is merged
    /// character by character when two devices have both changed it — right
    /// for a body of prose, and wrong for a list of objects in YAML, where an
    /// interleave produces something that is neither device's version and may
    /// not parse. A person's frontmatter was the largest such list in the app
    /// and grew with every coffee.
    ///
    /// One file each means two devices adding a coffee at once write two
    /// different files. There is nothing to merge, so there is nothing to get
    /// wrong.
    pub fn nodes_about_person(
        &self,
        person_id: &str,
        node_type: &str,
    ) -> AppResult<Vec<crate::models::node::NodeMetadata>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT n.id, n.node_type, n.title, n.content, n.properties,
                        n.created_at, n.updated_at, n.timestamp
                 FROM node_edges e
                 JOIN nodes n ON n.stable_id = e.source_id
                 WHERE e.edge_type = 'about'
                   AND n.node_type = ?2
                   AND e.target_id = COALESCE(
                       (SELECT stable_id FROM nodes WHERE id = ?1),
                       ?1
                   )
                 ORDER BY json_extract(n.properties, '$.date') DESC, n.created_at DESC",
            )
            .map_err(|e| {
                AppError::General(format!("DB Query Error (nodes_about_person): {}", e))
            })?;

        let rows = stmt
            .query_map(params![person_id, node_type], |row| {
                let props_str: String = row.get(4)?;
                Ok(crate::models::node::NodeMetadata {
                    id: row.get(0)?,
                    node_type: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    properties: serde_json::from_str(&props_str)
                        .unwrap_or(serde_json::Value::Null),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                    timestamp: row.get(7)?,
                    blocks: None,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (nodes_about_person): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    /// Who this person is linked to, as they stand right now.
    ///
    /// Read from the edge index rather than from the person's own
    /// frontmatter, and that is the whole point: the name and the path come
    /// from the other person's row, so a rename shows up everywhere at once
    /// and somebody deleted stops appearing at all. The frontmatter holds an
    /// identity and a relationship, and nothing that can go stale.
    pub fn person_connections(&self, person_id: &str) -> AppResult<Vec<PersonConnection>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT t.id, t.title, e.relation
                 FROM node_edges e
                 JOIN nodes s ON s.stable_id = e.source_id
                 JOIN nodes t ON t.stable_id = e.target_id
                 WHERE e.edge_type = 'person_link'
                   AND t.node_type = 'person'
                   AND s.id = ?1
                 ORDER BY t.title",
            )
            .map_err(|e| {
                AppError::General(format!("DB Query Error (person_connections): {}", e))
            })?;

        let rows = stmt
            .query_map(params![person_id], |row| {
                Ok(PersonConnection {
                    person_id: row.get(0)?,
                    name: row.get(1)?,
                    relation_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error (person_connections): {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn delete_node_edges_for_path(&self, rel_path: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_edges
                 WHERE source_id = ?1
                    OR source_id = COALESCE(
                        (SELECT stable_id FROM nodes WHERE id = ?1),
                        ?1
                    )",
                params![rel_path],
            )
            .map_err(|e| AppError::General(format!("DB Error clearing node_edges: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edges_by_source(&self, source_id: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM node_edges WHERE source_id = ?1",
                params![source_id],
            )
            .map_err(|e| AppError::General(format!("DB Error deleting node_edges: {}", e)))?;
        Ok(())
    }

    pub fn delete_node_edge(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM node_edges WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Error deleting node_edge: {}", e)))?;
        Ok(())
    }

    /// Get all edges connected to a node (both incoming and outgoing)
    pub fn get_node_edges_for_node(&self, node_id: &str) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at
             FROM node_edges
             WHERE source_id = ?1 OR target_id = ?1
             ORDER BY created_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Error querying node_edges: {}", e)))?;

        let rows = stmt
            .query_map(params![node_id], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }

    pub fn get_all_node_edges(&self) -> AppResult<Vec<NodeEdge>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, source_id, target_id, edge_type, relation, created_at FROM node_edges",
            )
            .map_err(|e| AppError::General(format!("DB Error querying all node_edges: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(NodeEdge {
                    id: row.get(0)?,
                    source_id: row.get(1)?,
                    target_id: row.get(2)?,
                    edge_type: row.get(3)?,
                    relation: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Error mapping all node_edges: {}", e)))?;

        Ok(rows.flatten().collect())
    }
}

impl DbBridge {
    /// The nodes that link to this one, by identity rather than by name.
    ///
    /// What this replaces was a `content LIKE '%filename%'` scan of every node
    /// in the vault. It was wrong in both directions at once: a file called
    /// `note.pdf` matched every note containing the word "note", while a file
    /// that had been renamed matched nothing at all even though the note still
    /// pointed at it. And it read the whole table to find out.
    ///
    /// Edges are recorded against stable identities and indexed on
    /// `target_id`, so this is a lookup, and renaming either end changes
    /// nothing about it.
    pub fn nodes_linking_to(
        &self,
        target_id: &str,
    ) -> AppResult<Vec<(String, String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT n.id, n.node_type, n.title, e.edge_type
                 FROM node_edges e
                 JOIN nodes n ON n.id = e.source_id OR n.stable_id = e.source_id
                 WHERE e.target_id = ?1
                 ORDER BY n.updated_at DESC",
            )
            .map_err(|e| AppError::General(format!("DB Query Error (backlinks): {}", e)))?;

        let rows = stmt
            .query_map(params![target_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error (backlinks): {}", e)))?;

        // One row per node, not per edge: a note that both embeds a picture and
        // links to it is one note that uses it, listed once.
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .flatten()
            .filter(|(id, _, _, _)| seen.insert(id.clone()))
            .collect())
    }
}

impl DbBridge {
    /// Point links that named something not yet indexed at the thing itself.
    ///
    /// A note is indexed when the vault is opened; the files it embeds are
    /// indexed when the Files app runs its scan. Whichever happens second, the
    /// first one has already written its links — and a note indexed first
    /// resolves `assets/so-do.png` to nothing, so the link is recorded against
    /// a placeholder: `ghost:assets/so-do.png`.
    ///
    /// Nothing re-reads the note afterwards, so without this the placeholder is
    /// permanent and the file's "used by" panel is empty for every file in the
    /// vault — which is exactly what it was.
    ///
    /// `UPDATE OR REPLACE` because the same note may already link to this node
    /// by another route, and `node_edges` holds one row per
    /// (source, target, type).
    pub fn adopt_ghost_edges(&self, names: &[String], node_id: &str) -> AppResult<usize> {
        if names.is_empty() {
            return Ok(0);
        }
        let ghosts: Vec<String> = names.iter().map(|n| format!("ghost:{}", n)).collect();
        let placeholders: Vec<String> = (1..=ghosts.len()).map(|i| format!("?{}", i + 1)).collect();

        let sql = format!(
            "UPDATE OR REPLACE node_edges SET target_id = ?1 WHERE target_id IN ({})",
            placeholders.join(", ")
        );
        let mut values: Vec<String> = vec![node_id.to_string()];
        values.extend(ghosts);

        let changed = self
            .conn
            .execute(&sql, rusqlite::params_from_iter(values.iter()))
            .map_err(|e| AppError::General(format!("DB Adopt Ghost Edges Error: {}", e)))?;
        Ok(changed)
    }

    /// Move links from one identity to another.
    ///
    /// For when the file at a path stops being the file it was: editing an
    /// embedded picture gives it new contents and therefore a new identity, and
    /// the note still points at the old one. The note means "the picture at
    /// this path", so the link follows the path.
    pub fn repoint_edges(&self, from: &str, to: &str) -> AppResult<usize> {
        if from == to {
            return Ok(0);
        }
        let changed = self
            .conn
            .execute(
                "UPDATE OR REPLACE node_edges SET target_id = ?1 WHERE target_id = ?2",
                params![to, from],
            )
            .map_err(|e| AppError::General(format!("DB Repoint Edges Error: {}", e)))?;
        Ok(changed)
    }
}
