import re

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'r') as f:
    content = f.read()

content = content.replace('- `[ ]` T3 - Application-level Keepalive (Ping/Pong)', '- `[x]` T3 - Application-level Keepalive (Ping/Pong)')
content = content.replace('  - `[ ]` Add `Ping` / `Pong` to `protocol.rs`', '  - `[x]` Add `Ping` / `Pong` to `protocol.rs`')
content = content.replace('  - `[ ]` Handle `Ping` in `sync-server/src/mailbox.rs`', '  - `[x]` Handle `Ping` in `sync-server/src/mailbox.rs`')
content = content.replace('  - `[ ]` Send `Ping` in `src-tauri/src/p2p/transport.rs` (Optional/Client side)', '  - `[x]` Send `Ping` in `src-tauri/src/p2p/transport.rs` (Skipped: Ephemeral Connection)')

content = content.replace('- `[ ]` T4 - Protocol Version Negotiation', '- `[/]` T4 - Protocol Version Negotiation')

with open('/Users/kid0604/.gemini/antigravity/brain/b7fd7895-4672-4afd-8151-ac04d8eb1246/task.md', 'w') as f:
    f.write(content)
