import { Extension, type KeyboardShortcutCommand } from '@tiptap/core';
import { invoke } from '@tauri-apps/api/core';
import { type as osType } from '@tauri-apps/plugin-os';
import { logger } from '../../../../utils/logger';

/**
 * Cmd+Shift+V: paste the clipboard's text and nothing else.
 *
 * Copying one column of a table on a web page (Confluence, say) leaves the
 * table on the clipboard as HTML, borders included, next to the same values as
 * plain text. An ordinary paste takes the HTML. A Shift paste takes the text,
 * one paragraph per line, and ProseMirror already does that. It just has to
 * receive the paste.
 *
 * On Windows it does, because WebView2 binds Ctrl+Shift+V itself. WKWebView
 * binds nothing to Cmd+Shift+V, so on a Mac the keys come to nothing. This
 * catches them there and has the webview do the native plain-text paste; see
 * `commands::paste` for why that goes through Rust instead of reading the
 * clipboard from script.
 *
 * Mac only, on purpose. Anywhere else, binding the keys here would take them
 * from a webview that already handles them properly.
 */
function onMac(): boolean {
  try {
    return osType() === 'macos';
  } catch {
    // Outside Tauri (tests, a plain browser) there is no OS plugin to ask.
    return false;
  }
}

function pasteAsPlainText(): boolean {
  invoke('paste_as_plain_text').catch((e) => logger.warn('Plain-text paste failed', e));
  return true;
}

export const PlainTextPaste = Extension.create({
  name: 'plainTextPaste',
  addKeyboardShortcuts(): Record<string, KeyboardShortcutCommand> {
    if (!onMac()) return {};
    return {
      'Mod-Shift-v': pasteAsPlainText,
      // What macOS itself calls Paste and Match Style.
      'Mod-Alt-Shift-v': pasteAsPlainText,
      // Control rather than Command, for hands that learned this on Windows.
      'Ctrl-Shift-v': pasteAsPlainText,
    };
  },
});
