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

`bundle.macOS.minimumSystemVersion` in `src-tauri/tauri.conf.json` is **13.3**,
the first macOS that ships Safari 16.4 — the floor Tailwind 4's CSS needs, and
the `build.target` Vite compiles for. Below it the app installed but drew broken
styles. Raising it is a decision; lowering it means proving the CSS still
renders on that Safari.

**The policy:**

- **Baseline Widely available** — use it, no fallback needed.
- **Baseline Newly available** — allowed *only* where the feature degrades to
  the previous behaviour on its own, with no fallback code. A macOS user cannot
  update their WebView without updating their OS, so "most people have it" is
  not the same claim here that it is on the web.
- **Anything needing a polyfill, or a fallback longer than ~20 lines** — pick a
  different approach.

The worked example is `content-visibility` in `TaskListView.vue`. It is Baseline
Newly available (September 2025; Safari 26, which macOS 13 never gets and
macOS 14–15 get only once Safari is updated), so a large share of macOS users
will not have it. That is fine, and only fine because a
WebView that does not know the property ignores it and renders every row, which
is exactly what the list did before. Had it needed a fallback to be correct
rather than merely fast, it would not have gone in.

`modern-web-guidance` carries the Baseline dates. Check there before reaching for
a platform feature rather than guessing from memory.
