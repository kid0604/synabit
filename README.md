# Synabit Productivity Suite

Synabit is a modern, ultra-fast, local-first productivity workspace. Designed to be your ultimate digital brain, it eliminates the need to jump between multiple apps by unifying your notes, tasks, calendar, and ideas into one seamless, cross-platform environment.

Whether you are a developer, student, or professional, Synabit keeps you focused, organized, and in complete control of your own data.

## Why Synabit?

- **All-in-One Digital Workspace**: No more context switching. Manage your deep-focus writing, quick fleeting ideas, daily tasks, calendar, and even RSS feeds or finances from a single beautiful interface.
- **Your vault is a folder of files**: Synabit writes plain Markdown with YAML frontmatter into a folder you choose. Your notes stay readable in any editor, diffable in git, and yours if Synabit ever disappears. What you make lives in the folder too, feed highlights, calendar subscriptions and timeline corrections included, so it is in every backup and on every synced device. The SQLite database beside it is an index rebuilt by scanning the folder; the one thing it holds that the folder does not is note version history. Settings that belong to one device (watched folders, reminders already shown, captures waiting for a vault to be open) are kept in a separate small file, so the index can be deleted without losing them. If the database is ever found corrupt, Synabit moves it aside rather than deleting it. Zero telemetry, no forced cloud account, no vendor lock-in.
- **End-to-end encrypted sync**: Each device talks to a store-and-forward mailbox server over QUIC (`iroh`). Everything is encrypted with XChaCha20-Poly1305 *before* it leaves your device, with a vault key that never does — the server holds blobs it cannot read.
- **AI on your own machine, by default**: Syn, the built-in assistant, talks to a local LLM through [Ollama](https://ollama.com). It can search your vault, create notes and tasks, and brief you on a person — without your notes leaving the machine. Cloud models (Anthropic, OpenAI, Gemini), web browsing, MCP connectors and a Telegram bot are opt-in; the app shows what each one sends, and Syn never reads notes or apps you have locked.

## Key Features

- **Note Vault**: A robust, block-based Markdown knowledge base with a rich-text editor for deep work. Notes are compound documents: they can host live query tables, whiteboards, PDFs with annotations, transclusions, maps and equations.
- **QuickCap**: A lightning-fast, masonry-layout tool for capturing fleeting ideas, images, and links instantly.
- **Whiteboard**: An infinite canvas for drawing, architecture diagrams, and visual thinking.
- **Task Management**: Tasks in four views — list, Kanban board, table and an Eisenhower matrix — with due times, repeats, reminders, subtasks and per-project budgets.
- **Syn**: An AI assistant with tool access to your vault — search, create, update, and summarise — running against a local Ollama model or, if you choose, a cloud one.
- **Things**: Browse every kind of node in the vault, with schemas and saved views.
- **Rich Table**: Typed columns, formulas, filters, board, pivot and chart views inside notes.
- **Timeline**: Dated moments drawn from your vault, on-this-day and a year view.
- **Safe**: A password manager with TOTP, imports from 1Password, Bitwarden and KeePass, and an SSH agent. Syn can use a secret without ever seeing it.
- **Nexus**: Full-text search and a graph of every node in the vault, including types Synabit has never heard of.
- **Smart Mini-Apps**: Built-in modules for managing RSS Feeds, Calendar, People (Contacts), and Personal Finance.
- **Drive / Files**: Integrated local file manager for the folders you point it at.

## Data & Security

Being precise about this matters more than sounding impressive:

| What | How it is protected |
| --- | --- |
| Data in transit (sync payloads, attachments) | XChaCha20-Poly1305, end-to-end. Keys are held in the OS keychain and never sent to a server. |
| App lock | Argon2id PIN hash, with optional per-app and per-note locking. A lock hides content from the screen and from Syn; it does not encrypt the file. |
| Analytics | There are none. No telemetry, no crash reporting, no phone-home. |
| **The vault and its index, at rest** | **Not encrypted by Synabit.** They are plain files on your disk. |

That last row is a deliberate trade, not an oversight: plain files are what make your vault readable by other tools, recoverable without Synabit, and diffable in git. If you need encryption at rest, use the one your OS already ships — FileVault, BitLocker, or LUKS — which protects the whole disk rather than one app's data.

Sync is peer-to-peer in the sense that matters — no account, no cloud provider holding your notes — but it is not serverless: devices exchange encrypted blobs through a Synabit mailbox server so that a device which was offline can still catch up. The server never holds a key.

## Tech Stack

Synabit is built for speed, beauty, and cross-platform compatibility (macOS, Windows, Linux, and Android):

- **Frontend**: Vue 3, Vite, Tailwind CSS, TypeScript
- **UI**: Tailwind CSS with a custom design system
- **Backend/Core**: Tauri 2.0 (Rust)
- **Storage**: A folder of Markdown/JSON files, indexed in SQLite (`rusqlite`) with FTS5 search
- **Sync**: `iroh` (QUIC transport), Loro (CRDT), XChaCha20-Poly1305 (E2EE)
- **AI**: Ollama (local models)

## Development

```bash
# Install dependencies
npm install

# Start development server
npm run tauri dev

# Build for release
npm run tauri build
```
