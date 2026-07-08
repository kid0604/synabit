import re

with open('src/db.rs', 'r') as f:
    content = f.read()

# Imports
content = content.replace('use rusqlite::{params, Connection, OptionalExtension};',
    'use rusqlite::{params, Connection, OptionalExtension};\nuse r2d2::Pool;\nuse r2d2_sqlite::SqliteConnectionManager;')
content = content.replace('use std::sync::{Arc, Mutex};', '')

# Struct Database
content = re.sub(r'pub struct Database \{\s*conn: Arc<Mutex<Connection>>,\s*\}',
    'pub struct Database {\n    pool: Pool<SqliteConnectionManager>,\n}', content)

# open()
open_replacement = """    pub fn open(path: &Path) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path)
            .with_init(|c| {
                c.execute_batch("PRAGMA journal_mode = WAL;")?;
                c.execute_batch("PRAGMA busy_timeout = 5000;")?;
                c.execute_batch("PRAGMA foreign_keys = ON;")
            });
        let pool = Pool::new(manager).context("failed to create connection pool")?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Run schema migrations
    fn migrate(&self) -> Result<()> {
        let conn = self.pool.get().context("pool get")?;
"""
content = re.sub(r'    pub fn open\(path: &Path\) -> Result<Self> \{.*?    fn migrate\(&self\) -> Result<\(\)> \{\n        let conn = self\.conn\.lock.*?\?;', open_replacement, content, flags=re.DOTALL)

# Replace all occurrences of self.conn.lock()
content = re.sub(r'let mut conn = self\.conn\.lock\(\)\.map_err\(\|e\| anyhow::anyhow!\("lock poisoned: \{e\}"\)\)\?;', 'let mut conn = self.pool.get().context("pool get")?;', content)
content = re.sub(r'let conn = self\.conn\.lock\(\)\.map_err\(\|e\| anyhow::anyhow!\("lock poisoned: \{e\}"\)\)\?;', 'let conn = self.pool.get().context("pool get")?;', content)

with open('src/db.rs', 'w') as f:
    f.write(content)
