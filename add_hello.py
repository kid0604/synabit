import re

def update_protocol(path):
    with open(path, 'r') as f:
        content = f.read()
    
    if 'Hello {' not in content:
        content = content.replace('pub enum MailboxRequest {\n', 'pub enum MailboxRequest {\n    /// Initial handshake to negotiate protocol version.\n    Hello {\n        version: u32,\n    },\n')
    if 'HelloOk {' not in content:
        content = content.replace('pub enum MailboxResponse {\n', 'pub enum MailboxResponse {\n    /// Handshake successful.\n    HelloOk {\n        server_version: u32,\n        max_bytes: u64,\n    },\n')
    
    with open(path, 'w') as f:
        f.write(content)

update_protocol('sync-server/src/protocol.rs')
update_protocol('src-tauri/src/sync/protocol.rs')
