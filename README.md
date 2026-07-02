# Synabit Productivity Suite

Synabit is a modern, ultra-fast, local-first productivity workspace. Designed to be your ultimate digital brain, it eliminates the need to jump between multiple apps by unifying your notes, tasks, calendar, and ideas into one seamless, cross-platform environment.

Whether you are a developer, student, or professional, Synabit keeps you focused, organized, and in complete control of your own data.

## Why Synabit?

- **All-in-One Digital Workspace**: No more context switching. Manage your deep-focus writing, quick fleeting ideas, daily tasks, calendar, and even RSS feeds or finances from a single beautiful interface.
- **True Local-First & Privacy**: Your data belongs to you. Everything is stored locally on your device inside an encrypted database. Zero telemetry, no forced cloud accounts, and no vendor lock-in.
- **Serverless P2P Sync**: Keep your devices in sync seamlessly. Synabit uses custom peer-to-peer (P2P) technology to securely transfer your data across devices over LAN—no central server required.
- **AI-Powered**: Integrate with your local LLMs (like Ollama) or cloud models to brainstorm, summarize, and assist you right inside your workspace without compromising privacy.

## Key Features

- **Note Vault**: A robust, block-based Markdown knowledge base with a rich-text editor for deep work.
- **QuickCap**: A lightning-fast, masonry-layout tool for capturing fleeting ideas, images, and links instantly.
- **Whiteboard**: An infinite canvas for drawing, architecture diagrams, and visual thinking.
- **Task Management**: Comprehensive task tracking with Kanban boards, Gantt charts, and Eisenhower matrices.
- **Smart Mini-Apps**: Built-in modules for managing RSS Feeds, Calendar, People (Contacts), and Personal Finance.
- **Drive / Files**: Integrated local file manager with Google Drive backup support.

## Tech Stack
Synabit is built for speed, beauty, and cross-platform compatibility (macOS, Windows, Linux, and Mobile):
- **Frontend**: Vue 3, Vite, Tailwind CSS, TypeScript
- **UI Framework**: Custom Modern Glassmorphism & Micro-animations
- **Backend/Core**: Tauri 2.0 (Rust)
- **Database**: SQLite (`rusqlite`)
- **Networking**: `iroh` (P2P Sync)

## Development
```bash
# Install dependencies
npm install

# Start development server
npm run tauri dev

# Build for release
npm run tauri build
```
