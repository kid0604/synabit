import re

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'r') as f:
    content = f.read()

content = content.replace('- `[/]` T5 - Server-Push Notification Setup', '- `[x]` T5 - Server-Push Notification Setup')
content = content.replace('  - `[ ]` Add `NotifyNewData` to `protocol.rs`', '  - `[x]` Add `NotifyNewData` to `protocol.rs`')
content = content.replace('  - `[ ]` Build VaultConnections Registry in `MailboxHandler`', '  - `[x]` Build VaultConnections Registry in `MailboxHandler`')
content = content.replace('  - `[ ]` Broadcast `NotifyNewData` on push events', '  - `[x]` Broadcast `NotifyNewData` on push events')

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'w') as f:
    f.write(content)
