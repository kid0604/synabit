import re

def update_protocol(path):
    with open(path, 'r') as f:
        content = f.read()
    
    if 'NotifyNewData {' not in content:
        content = content.replace('pub enum MailboxResponse {\n', 'pub enum MailboxResponse {\n    /// Server push notification: new data is available for this vault.\n    NotifyNewData {\n        /// The sequence number that triggered this notification.\n        trigger_seq: u64,\n    },\n')
    
    with open(path, 'w') as f:
        f.write(content)

update_protocol('sync-server/src/protocol.rs')
update_protocol('src-tauri/src/sync/protocol.rs')
