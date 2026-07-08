import re

with open('sync-server/src/mailbox.rs', 'r') as f:
    content = f.read()

notify_helper = """    /// Dispatch a single request within an authenticated session.
    
    async fn notify_subscribers(&self, vault_hash: &str, seq: u64) {
        let subs = self.active_subscriptions.read().await;
        if let Some(list) = subs.get(vault_hash) {
            for tx in list {
                let _ = tx.send(seq).await;
            }
        }
    }"""
    
content = content.replace('    /// Dispatch a single request within an authenticated session.', notify_helper)

# Update handle_push
content = content.replace('Ok(MailboxResponse::PushOk { seq })', 'self.notify_subscribers(vault_hash, seq).await;\n        Ok(MailboxResponse::PushOk { seq })')

# Update handle_push_delete
content = content.replace('Ok(MailboxResponse::DeleteOk { seq })', 'self.notify_subscribers(vault_hash, seq).await;\n        Ok(MailboxResponse::DeleteOk { seq })')

# Update handle_push_restore
content = content.replace('Ok(MailboxResponse::PushOk { seq })', 'self.notify_subscribers(vault_hash, seq).await;\n        Ok(MailboxResponse::PushOk { seq })')

# Note: handle_push_trash_meta does not generate a sequence number, so it doesn't need to notify? 
# Wait, trash meta does not trigger pull for clients, because it's not a doc entry.
# We will just notify on Push, PushDelete, PushRestore.

with open('sync-server/src/mailbox.rs', 'w') as f:
    f.write(content)

