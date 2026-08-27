# Working in this repository

## Browser support

Synabit is a Tauri app, so the front end does not run in a browser anyone
chooses. It runs in whatever WebView the operating system provides, and on two
of the three desktop platforms that is pinned to the OS itself:

| Platform | Engine | Updates with |
| --- | --- | --- |
| macOS | WKWebView | the OS — a user on an older macOS is on an older WebKit, permanently |
| Windows | WebView2 | itself, evergreen Chromium |
| Linux | WebKitGTK | the distribution's packages |
| Android | System WebView | Play Store, but stale on devices without Play Services (`minSdk = 24`) |

No `bundle.macOS.minimumSystemVersion` is declared in `src-tauri/tauri.conf.json`,
so the floor is Tauri's default rather than a considered decision. Treat the
supported range as wide until somebody narrows it on purpose.

**The policy:**

- **Baseline Widely available** — use it, no fallback needed.
- **Baseline Newly available** — allowed *only* where the feature degrades to
  the previous behaviour on its own, with no fallback code. A macOS user cannot
  update their WebView without updating their OS, so "most people have it" is
  not the same claim here that it is on the web.
- **Anything needing a polyfill, or a fallback longer than ~20 lines** — pick a
  different approach.

The worked example is `content-visibility` in `TaskListView.vue`. It is Baseline
Newly available (September 2025; Safari 26, so macOS 26), which means a large
share of macOS users will not have it. That is fine, and only fine because a
WebView that does not know the property ignores it and renders every row, which
is exactly what the list did before. Had it needed a fallback to be correct
rather than merely fast, it would not have gone in.

`modern-web-guidance` carries the Baseline dates. Check there before reaching for
a platform feature rather than guessing from memory.
