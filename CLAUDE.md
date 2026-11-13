# Working in this repository

## Browser support

Synabit is a Tauri app, so the front end does not run in a browser anyone
chooses. It runs in whatever WebView the operating system provides, and on two
of the three desktop platforms that is pinned to the OS itself:

| Platform | Engine | Updates with |
| --- | --- | --- |
| macOS | WKWebView | Safari updates, which reach the current macOS and the two before it; an older macOS is frozen on its last Safari |
| Windows | WebView2 | itself, evergreen Chromium |
| Linux | WebKitGTK | the distribution's packages |
| Android | System WebView | Play Store, but stale on devices without Play Services (`minSdk = 24`) |

`bundle.macOS.minimumSystemVersion` in `src-tauri/tauri.conf.json` is **14.4**,
the first macOS that ships Safari 17.4, and `build.target` in `vite.config.ts`
is `safari17.4` to match. It was 13.3 / Safari 16.4, the floor Tailwind 4's CSS
needs (below that the app installed but drew broken styles), until pdf.js raised
it on 2026-11-12; see below. Raising it is a decision; lowering it means proving
the CSS and pdf.js still work on that Safari.

**The policy:**

- **Baseline Widely available** — use it, no fallback needed.
- **Baseline Newly available** — allowed *only* where the feature degrades to
  the previous behaviour on its own, with no fallback code. A macOS user cannot
  update their WebView without updating their OS, so "most people have it" is
  not the same claim here that it is on the web.
- **Anything needing a polyfill, or a fallback longer than ~20 lines** — pick a
  different approach.

The worked example is `content-visibility` in `TaskListView.vue`. It is Baseline
Newly available (September 2025; Safari 26, which macOS 14–15 get only once
Safari is updated), so a large share of macOS users will not have it. That is fine, and only fine because a
WebView that does not know the property ignores it and renders every row, which
is exactly what the list did before. Had it needed a fallback to be correct
rather than merely fast, it would not have gone in.

The other example is pdf.js, the one dependency that sets the floor
(`src/shared/pdfjs.ts`; ESLint sends every `pdfjs-dist` import there):

- **The legacy build, never the standard one.** The standard build calls
  `Promise.try`, `URL.parse` and `Uint8Array#toHex` (Safari 18–18.2) to open any
  file. The legacy build is the same code with core-js added by Mozilla. That
  is allowed because we write and keep none of it, unlike a polyfill of our own.
- **Its floor is ours.** From pdf.js 6.1 the legacy build stops covering
  Safari 16.4: without `ArrayBuffer#transferToFixedLength` (17.4) pages draw
  with no text, and 6.5 drops `Promise.withResolvers` (17.4) too. Pinning 6.0
  would have frozen a parser of untrusted files, so the floor moved to 14.4
  instead. Before upgrading pdf.js, check what its legacy build stops covering.
- **What no Safari has, read around it.** `getTextContent()` iterates a
  `ReadableStream` with `for await`, which no Safari supports (WebKit bug
  194379). Use `pageText(page)`, or pass `page.streamTextContent()` to
  `TextLayer`. Both read with `getReader()`. A lint rule refuses
  `getTextContent`.

`modern-web-guidance` carries the Baseline dates. Check there before reaching for
a platform feature rather than guessing from memory.
