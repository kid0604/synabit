import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * A file waiting on its undo is hidden, not spliced out.
 *
 * Splicing it out of the sparse list shifted every later slot down by one, so
 * the next page fetched landed a row off: one file was drawn twice and the
 * one at the page boundary never at all.
 */

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke, convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));

import { useFileStore, type FileMetadata } from '../composables/useFileStore';

const TOTAL = 250;
const file = (i: number) => ({ id: `id-${i}`, path: `/v/f${i}.txt`, filename: `f${i}.txt`, extension: 'txt', size: 1 }) as unknown as FileMetadata;
const library = Array.from({ length: TOTAL }, (_, i) => file(i));
const settle = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };

describe('hiding a file for its undo window', () => {
  let deleted: Set<string>;

  beforeEach(() => {
    deleted = new Set();
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string, args: any) => {
      if (cmd === 'query_file_page') {
        const live = library.filter(f => !deleted.has(f.id));
        return { files: live.slice(args.offset, args.offset + args.limit), total: live.length };
      }
      if (cmd === 'delete_file') { deleted.add(args.fileId); return; }
      return [];
    });
  });

  it('keeps later pages at the offsets the database gave them', async () => {
    const store = useFileStore(() => '/v');
    await store.reload();
    store.hideFile(library[5]);

    expect(store.total.value).toBe(TOTAL - 1);
    expect(store.rows.value).toHaveLength(TOTAL - 1);

    // The reader scrolls to the end; every page arrives where it belongs.
    await store.ensureLoaded(0, TOTAL - 2);
    await settle();

    const shown = store.rows.value.map(f => f?.id);
    expect(shown).not.toContain('id-5');
    expect(shown.filter(Boolean)).toHaveLength(TOTAL - 1);
    expect(new Set(shown).size).toBe(TOTAL - 1);
    expect(shown[99]).toBe('id-100');
    expect(shown[TOTAL - 2]).toBe(`id-${TOTAL - 1}`);
  });

  it('stays hidden after its delete is written, until a reload asks again', async () => {
    const store = useFileStore(() => '/v');
    await store.reload();
    store.hideFile(library[5]);
    await store.deleteFile(library[5]);
    expect(store.rows.value.map(f => f?.id)).not.toContain('id-5');

    await store.reload();
    expect(store.total.value).toBe(TOTAL - 1);
    expect(store.rows.value.map(f => f?.id)).not.toContain('id-5');
  });

  it('comes back on Undo', async () => {
    const store = useFileStore(() => '/v');
    await store.reload();
    store.hideFile(library[5]);
    store.unhideFile(library[5]);
    expect(store.total.value).toBe(TOTAL);
    expect(store.rows.value[5]?.id).toBe('id-5');
  });
});
