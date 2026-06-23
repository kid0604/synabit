use rusqlite::params;
use crate::error::{AppError, AppResult};
use crate::models::file::{FileMetadata, FileSource};
use super::DbBridge;

impl DbBridge {
    pub fn upsert_file_source(&self, source: &FileSource) -> AppResult<()> {
        self.conn
            .execute(
                "INSERT INTO file_sources (id, path, name) 
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET 
                name=excluded.name",
                params![source.id, source.path, source.name],
            )
            .map_err(|e| AppError::General(format!("DB Upsert File Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_file_sources(&self) -> AppResult<Vec<FileSource>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, path, name FROM file_sources")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(FileSource {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    name: row.get(2)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut sources = Vec::new();
        for s in rows.flatten() {
            sources.push(s);
        }
        Ok(sources)
    }

    pub fn delete_file_source(&self, id: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM file_sources WHERE id = ?1", params![id])
            .map_err(|e| AppError::General(format!("DB Delete Source Error: {}", e)))?;
        Ok(())
    }

    pub fn upsert_file(&self, file: &FileMetadata) -> AppResult<()> {
        let tags_json = serde_json::to_string(&file.tags)?;
        self.conn.execute(
            "INSERT INTO files (id, path, filename, extension, size, created_at, modified_at, tags, source_type) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET 
                filename=excluded.filename,
                extension=excluded.extension,
                size=excluded.size,
                modified_at=excluded.modified_at",
            params![
                file.id, file.path, file.filename, file.extension, file.size, 
                file.created_at, file.modified_at, tags_json, file.source_type
            ],
        ).map_err(|e| AppError::General(format!("DB Upsert File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_file_by_path(&self, path: &str) -> AppResult<()> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", params![path])
            .map_err(|e| AppError::General(format!("DB Delete File Error: {}", e)))?;
        Ok(())
    }

    pub fn delete_files_by_source_type(&self, source_type: &str) -> AppResult<()> {
        self.conn
            .execute(
                "DELETE FROM files WHERE source_type = ?1",
                params![source_type],
            )
            .map_err(|e| AppError::General(format!("DB Delete Files by Source Error: {}", e)))?;
        Ok(())
    }

    pub fn get_all_files(&self) -> AppResult<Vec<FileMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, filename, extension, size, created_at, modified_at, tags, source_type 
             FROM files ORDER BY modified_at DESC"
        ).map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                let tags_str: String = row.get(7)?;
                let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();

                Ok(FileMetadata {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    extension: row.get(3)?,
                    size: row.get(4)?,
                    created_at: row.get(5)?,
                    modified_at: row.get(6)?,
                    tags,
                    people: vec![],
                    source_type: row.get(8)?,
                })
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        let mut files = Vec::new();
        for f in rows.flatten() {
            files.push(f);
        }
        Ok(files)
    }

    pub fn update_file_tags(&self, path: &str, tags: Vec<String>) -> AppResult<()> {
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        self.conn
            .execute(
                "UPDATE files SET tags = ?1 WHERE path = ?2",
                params![tags_json, path],
            )
            .map_err(|e| AppError::General(format!("DB Update File Tags Error: {}", e)))?;
        Ok(())
    }

    /// Read tags from the legacy `files` table for a given path.
    /// Returns None if the file doesn't exist or the table is gone.
    pub fn get_legacy_file_tags(&self, path: &str) -> Option<Vec<String>> {
        let tags_str: String = self
            .conn
            .query_row(
                "SELECT tags FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok()?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        if tags.is_empty() {
            None
        } else {
            Some(tags)
        }
    }

    pub fn update_file_path_and_name(
        &self,
        old_path: &str,
        new_path: &str,
        new_filename: &str,
        extension: &str,
    ) -> AppResult<()> {
        self.conn
            .execute(
                "UPDATE files SET path = ?1, filename = ?2, extension = ?3 WHERE path = ?4",
                params![new_path, new_filename, extension, old_path],
            )
            .map_err(|e| AppError::General(format!("DB Rename File Error: {}", e)))?;
        Ok(())
    }

    /// Search all node content (notes, tasks, events, whiteboards) for references to a filename.
    /// Returns (id, node_type, title) for each referencing node.
    pub fn find_nodes_referencing_file(
        &self,
        filename: &str,
    ) -> AppResult<Vec<(String, String, String)>> {
        let pattern = format!("%{}%", filename);
        let mut stmt = self
            .conn
            .prepare("SELECT id, node_type, title FROM nodes WHERE content LIKE ?1")
            .map_err(|e| AppError::General(format!("DB Query Error: {}", e)))?;

        let rows = stmt
            .query_map(params![pattern], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| AppError::General(format!("DB Map Error: {}", e)))?;

        Ok(rows.flatten().collect())
    }
}
