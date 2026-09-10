import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Table, TableRow, TableCell, TableHeader } from '@tiptap/extension-table';
import { invoke } from '@tauri-apps/api/core';
import { type as osType } from '@tauri-apps/plugin-os';
import { PlainTextPaste } from '../extensions/plainTextPaste';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn() }));
vi.mock('../../../../utils/logger', () => ({ logger: { warn: vi.fn() } }));

let editor: Editor;

function build(os: string) {
  vi.mocked(osType).mockReturnValue(os as ReturnType<typeof osType>);
  editor = new Editor({
    element: document.createElement('div'),
    extensions: [StarterKit, Table, TableRow, TableCell, TableHeader, PlainTextPaste],
    content: '<p></p>',
  });
}

// jsdom has no Mac, so ProseMirror reads Mod as Control here.
function pressMod(key: string, { shift = false } = {}) {
  const event = new KeyboardEvent('keydown', {
    key: shift ? key.toUpperCase() : key, keyCode: key.toUpperCase().charCodeAt(0),
    ctrlKey: true, shiftKey: shift, bubbles: true, cancelable: true,
  });
  editor.view.dom.dispatchEvent(event);
  return event;
}
const pressShiftV = () => pressMod('v', { shift: true });

function paste(data: Record<string, string>) {
  const event = new Event('paste', { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'clipboardData', {
    value: { getData: (t: string) => data[t] ?? '', types: Object.keys(data), items: [] },
  });
  editor.view.dom.dispatchEvent(event);
}

// One column of a Confluence table, as a browser puts it on the clipboard.
const column = {
  'text/html': '<table><tr><td><p>A1</p></td></tr><tr><td><p>A2</p></td></tr></table>',
  'text/plain': 'A1\nA2',
};

beforeEach(() => vi.clearAllMocks());
afterEach(() => editor.destroy());

describe('PlainTextPaste', () => {
  it('asks the webview for a plain-text paste on macOS', () => {
    build('macos');
    const event = pressShiftV();
    expect(invoke).toHaveBeenCalledWith('paste_as_plain_text');
    expect(event.defaultPrevented).toBe(true);
  });

  it('leaves the keys to the webview everywhere else', () => {
    build('windows');
    const event = pressShiftV();
    expect(invoke).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  // The native command delivers text/plain alone. What the editor makes of it
  // is the point of the whole shortcut: the values, without the table.
  it('turns a text-only paste of a column into one paragraph per value', () => {
    build('macos');
    paste({ 'text/plain': column['text/plain'] });
    const html = editor.getHTML();
    expect(html).not.toContain('<table');
    expect(html).toContain('<p>A1</p><p>A2</p>');
  });

  // A paste somebody did not mean is undone like any other edit, and the
  // native command leaves nothing behind for the menu's Undo to trip over.
  it('can be undone and redone', () => {
    build('macos');
    paste({ 'text/plain': column['text/plain'] });
    expect(editor.getHTML()).toContain('<p>A1</p><p>A2</p>');

    expect(pressMod('z').defaultPrevented).toBe(true);
    expect(editor.getHTML()).toBe('<p></p>');

    pressMod('z', { shift: true });
    expect(editor.getHTML()).toContain('<p>A1</p><p>A2</p>');
  });

  it('still pastes the table when the HTML comes along', () => {
    build('macos');
    paste(column);
    expect(editor.getHTML()).toContain('<table');
  });
});
