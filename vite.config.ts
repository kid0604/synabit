import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue(), tailwindcss()],

  build: {
    // The oldest WebView this app has to run in, not a browser list. Tailwind 4
    // already needs Safari 16.4 and Chrome 111 for the CSS it emits, so that is
    // the floor in practice — macOS's WKWebView being the one that cannot be
    // updated without updating the OS (see CLAUDE.md).
    target: ['es2020', 'safari16.4', 'chrome111'],
    rollupOptions: {
      output: {
        // The editor's engine — Tiptap's core and ProseMirror, which the note
        // editor, the whiteboard and the chat all share — as one chunk with a
        // name, instead of whatever module Rollup named the shared chunk after:
        // it was `edgeMarkers`, a whiteboard file. Only the engine: Tiptap's
        // extensions are left where Rollup puts them, so a screen that uses
        // three of them does not load all thirty. Mermaid needs no entry here: it is loaded on demand
        // (`shared/mermaid.ts`) and its chunk is already `mermaid.core`.
        manualChunks(id) {
          if (/[\\/]node_modules[\\/](@tiptap[\\/](core|pm|vue-3)[\\/]|prosemirror-(?!markdown)|rope-sequence[\\/]|orderedmap[\\/]|w3c-keyname[\\/])/.test(id)) {
            return 'tiptap';
          }
          // Vue named too, or Rollup folds it into the `tiptap` chunk (the
          // editor's Vue bindings depend on it) and every window then loads
          // the editor's engine just to get Vue.
          if (/[\\/]node_modules[\\/](vue|@vue)[\\/]/.test(id)) {
            return 'vue';
          }
        },
      },
    },
  },
  optimizeDeps: {
    include: [
      'vue',
      'vue-router',
      'pinia',
      'lucide-vue-next',
      'd3',
      'marked',
      'dompurify',
      '@tauri-apps/api/core',
      '@tauri-apps/api/event',
      '@tauri-apps/api/window',
      '@tauri-apps/plugin-store'
    ]
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || "0.0.0.0",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
