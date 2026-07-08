import re

with open('sync-server/src/mailbox.rs', 'r') as f:
    content = f.read()

struct_old = """pub struct MailboxHandler {
    db: Database,
    config: AppConfig,
    blob_dir: PathBuf,
    endpoint_id: RwLock<String>,
    /// Per-vault concurrent connection counter for basic rate limiting.
    concurrent_connections: Arc<Mutex<HashMap<String, u32>>>,
}"""

struct_new = """pub struct MailboxHandler {
    db: Database,
    config: AppConfig,
    blob_dir: PathBuf,
    endpoint_id: RwLock<String>,
    /// Per-vault concurrent connection counter for basic rate limiting.
    concurrent_connections: Arc<Mutex<HashMap<String, u32>>>,
    /// Registry of active connections waiting for push notifications.
    /// Maps vault_hash (hex) to a list of channels.
    active_subscriptions: Arc<tokio::sync::RwLock<HashMap<String, Vec<tokio::sync::mpsc::Sender<u64>>>>>,
}"""

content = content.replace(struct_old, struct_new)

new_old = """        Ok(Self {
            db,
            config,
            blob_dir,
            endpoint_id: RwLock::new(String::new()),
            concurrent_connections: Arc::new(Mutex::new(HashMap::new())),
        })"""

new_new = """        Ok(Self {
            db,
            config,
            blob_dir,
            endpoint_id: RwLock::new(String::new()),
            concurrent_connections: Arc::new(Mutex::new(HashMap::new())),
            active_subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        })"""

content = content.replace(new_old, new_new)

with open('sync-server/src/mailbox.rs', 'w') as f:
    f.write(content)

