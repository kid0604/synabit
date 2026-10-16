import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}` }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true), message: vi.fn(async () => undefined) }));
vi.mock('../../../../utils/logger', () => ({ logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));

import { mountNoteEditor, type NoteEditor } from '../../../../audit/noteEditor';

/**
 * Opening a note a hundred times leaves the parser as opening it once did.
 *
 * Every parse runs every extension's setup on the same markdown-it, and one
 * that adds a rule each time grows the parser for as long as the app runs:
 * tiptap-markdown's task lists did, 52 rules after 50 parses, and the equation
 * did until it was guarded. Found when the note property test ran out of
 * memory, not by anybody noticing a slow week.
 */
let note: NoteEditor;
beforeAll(async () => {
  note = await mountNoteEditor();
});
afterAll(() => note.wrapper.unmount());

const rules = (md: any) =>
  ['core', 'block', 'inline', 'inline2'].reduce((n, chain) => n + (md[chain]?.ruler?.__rules__?.length ?? 0), 0);

describe('opening notes, again and again', () => {
  it('does not grow the parser', () => {
    const md = (note.editor.storage as any).markdown.parser.md;
    const sample = '- [ ] việc\n\n$$x^2$$ và $y$\n\n<details><summary>T</summary>\n\nx\n\n</details>\n';
    note.open(sample);
    const once = rules(md);
    for (let i = 0; i < 100; i++) note.open(sample);
    expect(rules(md)).toBe(once);
  });
});
