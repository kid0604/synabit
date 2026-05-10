# Third-Party Licenses — Synabit

This file lists the open-source libraries used by Synabit along with their
respective licenses. We are grateful to the authors and contributors of
these projects.

Last generated: May 10, 2026

---

## Rust Dependencies (Backend)

| Package | Version | License |
|---|---|---|
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| chrono | 0.4.44 | MIT OR Apache-2.0 |
| dirs | 6.0.0 | MIT OR Apache-2.0 |
| dotenvy | 0.15.7 | MIT |
| gray_matter | 0.3.2 | MIT |
| infer | 0.19.0 | MIT |
| keyring | 3.6.3 | MIT OR Apache-2.0 |
| log | 0.4.29 | MIT OR Apache-2.0 |
| notify | 6.1.1 | CC0-1.0 |
| oauth2 | 5.0.0 | MIT OR Apache-2.0 |
| opener | 0.8.4 | MIT OR Apache-2.0 |
| rand | 0.9.4 | MIT OR Apache-2.0 |
| regex | 1.12.3 | MIT OR Apache-2.0 |
| reqwest | 0.12.28 | MIT OR Apache-2.0 |
| rusqlite | 0.39.0 | MIT |
| serde | 1.0.228 | MIT OR Apache-2.0 |
| serde_json | 1.0.149 | MIT OR Apache-2.0 |
| serde_yaml | 0.9.34 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| tauri | 2.10.3 | Apache-2.0 OR MIT |
| tauri-build | 2.5.6 | Apache-2.0 OR MIT |
| tauri-plugin-deep-link | 2.4.7 | Apache-2.0 OR MIT |
| tauri-plugin-dialog | 2.7.0 | Apache-2.0 OR MIT |
| tauri-plugin-fs | 2.5.0 | Apache-2.0 OR MIT |
| tauri-plugin-log | 2.8.0 | Apache-2.0 OR MIT |
| tauri-plugin-opener | 2.5.3 | Apache-2.0 OR MIT |
| tauri-plugin-os | 2.3.2 | Apache-2.0 OR MIT |
| tauri-plugin-store | 2.4.2 | Apache-2.0 OR MIT |
| thiserror | 2.0.18 | MIT OR Apache-2.0 |
| tokio | 1.51.0 | MIT |
| urlencoding | 2.1.3 | MIT |
| uuid | 1.23.1 | Apache-2.0 OR MIT |
| walkdir | 2.5.0 | Unlicense OR MIT |

> Note: Transitive dependencies (not listed individually) include crates
> licensed under MIT, Apache-2.0, CC0-1.0, Unlicense, and MPL-2.0.
> The MPL-2.0 crates (cssparser, selectors, option-ext, dtoa-short) are
> used as-is without modification; no copyleft obligations are triggered.

---

## JavaScript Dependencies (Frontend)

### Core Frameworks

| Package | Version | License |
|---|---|---|
| vue | 3.x | MIT |
| vue-router | 4.x | MIT |
| @tauri-apps/api | 2.10.1 | Apache-2.0 OR MIT |
| tailwindcss | 4.2.2 | MIT |

### Editor (TipTap)

| Package | Version | License |
|---|---|---|
| @tiptap/core | 3.22.3 | MIT |
| @tiptap/starter-kit | 3.22.2 | MIT |
| @tiptap/pm (ProseMirror) | 3.22.3 | MIT |
| @tiptap/extension-* (all) | 3.22.x | MIT |
| @tiptap/suggestion | 3.22.2 | MIT |

### Visualization

| Package | Version | License |
|---|---|---|
| d3 | 7.x | ISC |
| @vue-flow/core | latest | MIT |
| mermaid | latest | MIT |

### Tauri Plugins (Frontend SDKs)

| Package | Version | License |
|---|---|---|
| @tauri-apps/plugin-deep-link | 2.4.8 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-dialog | 2.7.0 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-fs | 2.5.0 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-log | 2.8.0 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-opener | 2.5.3 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-os | 2.3.2 | MIT OR Apache-2.0 |
| @tauri-apps/plugin-store | 2.4.2 | MIT OR Apache-2.0 |

### UI & Utilities

| Package | Version | License |
|---|---|---|
| @iconify/vue | latest | MIT |
| @floating-ui/dom | 1.7.6 | MIT |
| @popperjs/core | 2.11.8 | MIT |
| sortablejs | latest | MIT |
| vuedraggable | latest | MIT |
| lowlight | latest | BSD-3-Clause |
| highlight.js | latest | BSD-3-Clause |

### Build Tools

| Package | Version | License |
|---|---|---|
| vite | 6.x | MIT |
| @vitejs/plugin-vue | latest | MIT |
| typescript | 5.x | Apache-2.0 |
| vue-tsc | latest | MIT |
| lightningcss | 1.32.0 | MPL-2.0 |

> Note: lightningcss is licensed under MPL-2.0 (weak copyleft). It is used
> as a build-time CSS compiler without modification. No copyleft obligations
> are triggered for the output CSS or the application source code.

---

## License Texts

### MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.

### Apache License 2.0

Licensed under the Apache License, Version 2.0. You may obtain a copy at
http://www.apache.org/licenses/LICENSE-2.0

### ISC License

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

### Mozilla Public License 2.0 (MPL-2.0)

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. A copy of the MPL is available at https://mozilla.org/MPL/2.0/

### CC0 1.0 Universal

The person who associated a work with this deed has dedicated the work to the
public domain by waiving all of his or her rights to the work worldwide under
copyright law.

### BSD 3-Clause License

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met.
See https://opensource.org/licenses/BSD-3-Clause for full text.

### Unlicense

This is free and unencumbered software released into the public domain.
See https://unlicense.org for full text.

---

*This file is auto-generated and may be updated with each release.
For the most accurate and up-to-date information, run:*
- *Rust: `cargo tree --format "{p} {l}"`*
- *npm: `npx license-checker --production --summary`*
