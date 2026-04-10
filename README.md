# Synabit Productivity Suite

Synabit is a modern, ultra-fast productivity workspace built for macOS. Combining the power of a Markdown-based Zettelkasten knowledge vault with high-speed fleeting notes (QuickCaps), Synabit is designed to keep you focused.

## Features (v0.0.1)
- **Note Vault**: A robust Markdown note management system. Notes are stored securely in your local `Notes/` folder.
- **QuickCap**: A lightning-fast fleeting note tool inspired by Google Keep and Apple Notes.
  - Masonry layout for variable-height cards.
  - Image pasting (`Cmd+V`) support directly into notes.
  - Inline, Bear.app style tagging system (e.g., `#idea#`) supporting multi-word tags.
  - Real-time text search and `#tag` filtering.
- **Privacy First**: Everything is stored as local `.md` files on your device, not on the cloud.

## Tech Stack
- **Frontend**: Vue 3, Vite, Tailwind CSS, TypeScript
- **UI Framework**: Vanilla CSS with Modern Glassmorphism and Lucide Icons
- **Backend/Core**: Tauri (Rust Platform)

## Development
```bash
# Install dependencies
npm install

# Start development server
npm run tauri dev

# Build for release
npm run tauri build
```
