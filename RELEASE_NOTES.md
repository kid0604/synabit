# Release Notes

## Version 0.0.1
**Initial Milestone Release**

### Added
- **Core Engine & Vault:**
  - Integrated local Vault path selection and persistence.
  - Configured Rust directory traversal to strictly separate `Notes/` and `QuickCaps/` to prevent scope contamination.
  - Automatic directory migration for unorganized root files upon boot.
- **Note App:**
  - Basic list execution, read, and write for Zettelkasten-style permanent notes.
  - YAML FrontMatter parser integrated via Rust backend.
- **QuickCap App:**
  - Fully responsive Masonry Layout for fleeting notes.
  - "Full View" Modal to seamlessly read long notes.
  - **Image Pasting**: Native intercept for `Cmd+V` clipboard blobs. Automatically saves to `Vault/assets` and displays markdown `<img>` dynamically in Vue.
  - **Real-time Filter & Search**: A highly responsive search bar handling normal text searches or contextual `#tag` lookups.
  - **Advanced Tagging System**: Support for traditional single-word tags (`#idea`) and Bear-style multi-word wrapped tags (`#review sách#`).
  - Instant UI Label chips generated dynamically from tags, hiding Markdown hash characters for visual elegance.
- **Security & Stability:**
  - Integrated `@tauri-apps/plugin-dialog` to safely confirm non-reversible deletion operations.
  - Markdown XSS sanitization built right into the `v-html` renderer for absolute UI safety.
