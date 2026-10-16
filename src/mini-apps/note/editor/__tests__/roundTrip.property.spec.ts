import { describe, it, expect, beforeAll, afterAll, vi } from 'vitest';
import fc from 'fast-check';
import type { Node as PMNode } from '@tiptap/pm/model';

// As Tauri makes a file's address, so saving finds and undoes it (`useAssetPaths`).
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}` }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true), message: vi.fn(async () => undefined) }));
vi.mock('../../../../utils/logger', () => ({ logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));

import { mountNoteEditor, type NoteEditor } from '../../../../audit/noteEditor';
import { noteDoc } from '../../../../audit/noteArbitraries';
import { lostOnReopen } from '../roundTrip';

/**
 * `note.roundtrip`, over notes nobody wrote: random documents in the real
 * editor's schema, saved, opened again, saved again.
 *
 * What may change is what markdown itself normalises — two lists side by side
 * become one, so lists, quotes and paragraphs are not counted. Everything a
 * person put there must come back: every word and link, every heading at its
 * level, every image, table cell, list item, tick, embed, equation and line
 * break, every word's bold or italic. And saving must settle: a note that
 * changes each time it is opened is written, and synced, each time.
 *
 * `NOTE_PROPERTY_RUNS` runs more (the default keeps the suite quick); a
 * failure prints the seed and the smallest document that fails.
 */

declare const process: { env: Record<string, string | undefined> } | undefined;
const RUNS = Number((typeof process !== 'undefined' && process?.env.NOTE_PROPERTY_RUNS) || 150);

/** What a note must not lose, counted. */
export function signature(doc: PMNode): Record<string, number> {
  const count: Record<string, number> = {};
  const add = (k: string) => (count[k] = (count[k] ?? 0) + 1);
  const NOT_COUNTED = new Set(['doc', 'text', 'paragraph', 'bulletList', 'orderedList', 'taskList', 'blockquote', 'table', 'tableRow']);
  doc.descendants((n) => {
    if (!NOT_COUNTED.has(n.type.name)) {
      add(n.type.name === 'heading' ? `heading${n.attrs.level}` : n.type.name === 'taskItem' ? `taskItem:${n.attrs.checked}` : n.type.name);
    }
    if (n.isText) {
      for (const w of (n.text ?? '').split(/\s+/).filter(Boolean)) for (const m of n.marks) add(`${m.type.name}:word`);
    }
    return true;
  });
  return count;
}

/**
 * `after` as `before` should equal it, allowing for what opening adds on
 * purpose: an address typed as text is made a link (autolink), so linked words
 * may only grow.
 */
function asOpened(after: Record<string, number>, before: Record<string, number>): Record<string, number> {
  const out = { ...after };
  if ((out['link:word'] ?? 0) > (before['link:word'] ?? 0)) {
    if (before['link:word']) out['link:word'] = before['link:word'];
    else delete out['link:word'];
  }
  return out;
}

let note: NoteEditor;
beforeAll(async () => {
  note = await mountNoteEditor();
});
afterAll(() => note.wrapper.unmount());

describe('a note, saved and opened again', () => {
  it('keeps what was in it, and saving settles', async () => {
    await fc.assert(
      fc.asyncProperty(noteDoc, async (doc) => {
        const { editor, open, save } = note;
        editor.commands.setContent(doc as any);
        const typed = editor.state.doc;
        const first = save();

        const lost = lostOnReopen(editor, typed, first);
        expect(lost, `lost on reopening:\n${first}`).toEqual({ words: 0, links: 0, firstBlock: null });

        open(first);
        expect(asOpened(signature(editor.state.doc), signature(typed)), `opened differently:\n${first}`).toEqual(signature(typed));

        // Settled from the second save on. The first may still change once:
        // a list marked tight whose item has two paragraphs cannot be written
        // tight, comes back loose, and is written loose from then on. One
        // extra write. What may not happen is a change every time — the
        // space an equation gained on each save did.
        const second = save();
        open(second);
        const third = save();
        expect(third, `saved differently each time:\n${second}`).toBe(second);

        // A turn of the event loop between notes, as the app has between
        // opening one and the next: Vue tidies away the node views it
        // replaced then, and a loop that never yields holds every one.
        await new Promise((r) => setTimeout(r, 0));
      }),
      { numRuns: RUNS },
    );
  }, Math.max(120_000, RUNS * 250));
});
