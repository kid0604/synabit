import re

with open('src/db.rs', 'r') as f:
    content = f.read()

# 1. Imports
content = content.replace('use rusqlite::{params, Connection, OptionalExtension};', 
    'use rusqlite::{params, Connection, OptionalExtension};\nuse deadpool_sqlite::{Config, Runtime, Pool};')
content = content.replace('use std::sync::{Arc, Mutex};', '')

# 2. Struct Database
content = re.sub(r'pub struct Database \{\s*conn: Arc<Mutex<Connection>>,\s*\}', 
    'pub struct Database {\n    pool: Pool,\n}', content)

# 3. open()
open_replacement = """    pub async fn open(path: &Path) -> Result<Self> {
        let cfg = Config::new(path);
        let pool = cfg.create_pool(Runtime::Tokio1).context("failed to create db pool")?;
        
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    /// Run schema migrations
    async fn migrate(&self) -> Result<()> {
        let conn = self.pool.get().await.context("get connection")?;
        conn.interact(|conn| {
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
"""

content = re.sub(r'    pub fn open\(path: &Path\) -> Result<Self> \{.*?Ok\(\(\)\)\n    \}', open_replacement + content[content.find('            CREATE TABLE IF NOT EXISTS vaults'):content.find('Ok(())', content.find('            CREATE TABLE IF NOT EXISTS vaults'))] + "        }).await.map_err(|e| anyhow::anyhow!(\"interact error: {e}\"))??;\n        Ok(())\n    }", content, flags=re.DOTALL)

with open('src/db.rs', 'w') as f:
    f.write(content)
