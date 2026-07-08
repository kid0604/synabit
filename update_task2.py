import re

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'r') as f:
    content = f.read()

content = content.replace('- `[/]` T4 - Protocol Version Negotiation', '- `[x]` T4 - Protocol Version Negotiation')
content = content.replace('  - `[ ]` Add `Hello` / `HelloOk` to `protocol.rs`', '  - `[x]` Add `Hello` / `HelloOk` to `protocol.rs`')
content = content.replace('  - `[ ]` Implement handshake logic in server and client', '  - `[x]` Implement handshake logic in server and client')

content = content.replace('- `[ ]` T5 - Server-Push Notification Setup', '- `[/]` T5 - Server-Push Notification Setup')

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'w') as f:
    f.write(content)
