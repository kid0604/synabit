import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import NoteApp from '../NoteApp.vue';
import * as core from '@tauri-apps/api/core';
import * as dialog from '@tauri-apps/plugin-dialog';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn()
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  ask: vi.fn().mockResolvedValue(true),
  message: vi.fn().mockResolvedValue(undefined)
}));
// Only `useI18n` is stubbed. Replacing the whole module would take
// `createI18n` with it, which `src/i18n/index.ts` calls at import time by way
// of `useSettings` — the suite then fails before a single test runs.
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key })
}));

describe('NoteApp.vue', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('scans the vault on mount if vaultPath is provided', async () => {
    // Summaries, not whole notes: the list renders a preview and never the
    // body, so the body is not sent.
    const mockSummaries = [
      {
        id: 'Notes/note1.md',
        node_type: 'note',
        title: 'Test Note 1',
        preview: 'hello',
        properties: { tags: ['work'], pinned: false, full_width: false },
        created_at: '2026-05-01 00:00:00',
        updated_at: '2026-05-01 00:00:00',
        timestamp: 1746057600000
      }
    ];

    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'get_node_summaries') return Promise.resolve(mockSummaries);
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
      return Promise.resolve();
    });

    const wrapper = mount(NoteApp, {
      props: {
        vaultPath: '/mock/vault'
      },
      global: {
        plugins: [createTestingPinia({ createSpy: vi.fn })],
        stubs: {
          TiptapEditor: true,
          NoteGraph: true,
          'lucide-vue-next': true
        },
        mocks: {
          $t: (key: string) => key
        }
      }
    });

    // Wait for async operations to complete
    await new Promise(resolve => setTimeout(resolve, 100));

    // The list asks for summaries, and must not ask for whole notes.
    expect(core.invoke).toHaveBeenCalledWith('get_node_summaries', { nodeType: 'note' });
    expect(core.invoke).not.toHaveBeenCalledWith('get_nodes', { nodeType: 'note' });

    // Component exposes notes array
    const exposed = wrapper.vm as any;
    expect(exposed.notes.length).toBe(1);
    expect(exposed.notes[0].title).toBe('Test Note 1');
    expect(exposed.notes[0].summary).toBe('hello');
    expect(exposed.notes[0].content).toBeUndefined();
    
    // Automatically selects the first note if none is selected
    expect(exposed.currentNoteId).toBe('Notes/note1.md');
  });

  it('asks before deleting, and does nothing at all if the answer is no', async () => {
    const mockSummaries = [{
      id: 'Notes/note1.md', node_type: 'note', title: 'Test Note 1', preview: 'hello',
      properties: { tags: [], pinned: false, full_width: false },
      created_at: '2026-05-01 00:00:00', updated_at: '2026-05-01 00:00:00',
      timestamp: 1746057600000,
    }];
    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'get_node_summaries') return Promise.resolve(mockSummaries);
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
      return Promise.resolve();
    });
    vi.mocked(dialog.ask).mockResolvedValueOnce(false);

    const wrapper = mount(NoteApp, {
      props: { vaultPath: '/mock/vault' },
      global: {
        plugins: [createTestingPinia({
          createSpy: vi.fn,
          initialState: { app: { vaultPath: '/mock/vault' } },
        })],
        stubs: { TiptapEditor: true, NoteGraph: true, 'lucide-vue-next': true },
        mocks: { $t: (key: string) => key },
      },
    });
    await new Promise(resolve => setTimeout(resolve, 100));
    const exposed = wrapper.vm as any;

    await exposed.deleteNote('Notes/note1.md');
    await new Promise(resolve => setTimeout(resolve, 50));

    expect(dialog.ask).toHaveBeenCalled();
    // Declining leaves the note exactly where it was.
    expect(exposed.notes.length).toBe(1);
    expect(core.invoke).not.toHaveBeenCalledWith('trash_node_file', expect.anything());
  });

  it('takes a deleted note off the list, and never unlinks the file', async () => {
    // The note the delete is aimed at. It has to be in the list for the rest
    // of the teardown — tabs, recents — to have anything to act on.
    const mockSummaries = [
      {
        id: 'Notes/note1.md',
        node_type: 'note',
        title: 'Test Note 1',
        preview: 'hello',
        properties: { tags: [], pinned: false, full_width: false },
        created_at: '2026-05-01 00:00:00',
        updated_at: '2026-05-01 00:00:00',
        timestamp: 1746057600000
      }
    ];

    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'get_node_summaries') return Promise.resolve(mockSummaries);
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
      if (cmd === 'trash_node_file') return Promise.resolve('.trash/Notes/note1.md');
      return Promise.resolve();
    });

    const wrapper = mount(NoteApp, {
      props: { vaultPath: '/mock/vault' },
      global: {
        // `useNodeService` takes the vault from the store rather than the
        // prop, so the store is the one that has to know where it is.
        plugins: [createTestingPinia({
          createSpy: vi.fn,
          initialState: { app: { vaultPath: '/mock/vault' } }
        })],
        stubs: { TiptapEditor: true, NoteGraph: true, 'lucide-vue-next': true },
        mocks: { $t: (key: string) => key }
      }
    });

    await new Promise(resolve => setTimeout(resolve, 100));
    const exposed = wrapper.vm as any;
    expect(exposed.notes.length).toBe(1);

    await exposed.deleteNote('Notes/note1.md');
    await new Promise(resolve => setTimeout(resolve, 50));

    // Gone from the list straight away, but nothing has happened on disk yet:
    // the file is only moved once the undo window closes, which is what makes
    // undo a cancelled timer rather than a race against sync's tombstone.
    // `useNoteDelete.spec.ts` drives that timing; this covers the wiring.
    expect(exposed.notes.length).toBe(0);
    expect(core.invoke).not.toHaveBeenCalledWith('trash_node_file', expect.anything());

    // And whatever else happens, a note is never simply unlinked. It is
    // usually the only copy of what somebody wrote.
    expect(core.invoke).not.toHaveBeenCalledWith('delete_node_file', expect.anything());
  });

  /**
   * The manager is the only list that shows every note, so it is the only
   * place worth ticking rows in. This drives the wiring end to end — the
   * composables have their own tests; what this covers is that the table is
   * actually connected to them.
   */
  it('ticks rows in the manager and deletes the lot in one go', async () => {
    const summary = (n: number) => ({
      id: `Notes/note${n}.md`,
      node_type: 'note',
      title: `Note ${n}`,
      preview: '',
      properties: { tags: [], pinned: false, full_width: false },
      created_at: '2026-05-01 00:00:00',
      updated_at: '2026-05-01 00:00:00',
      timestamp: 1746057600000 - n,
    });

    vi.mocked(core.invoke).mockImplementation((cmd) => {
      if (cmd === 'get_node_summaries') return Promise.resolve([summary(1), summary(2), summary(3)]);
      if (cmd === 'get_linked_nodes') return Promise.resolve([]);
      if (cmd === 'trash_node_file') return Promise.resolve('.trash/x');
      return Promise.resolve();
    });
    vi.mocked(dialog.ask).mockResolvedValue(true);

    const wrapper = mount(NoteApp, {
      props: { vaultPath: '/mock/vault' },
      global: {
        plugins: [createTestingPinia({
          createSpy: vi.fn,
          initialState: { app: { vaultPath: '/mock/vault' } },
        })],
        stubs: { TiptapEditor: true, NoteGraph: true, 'lucide-vue-next': true },
        mocks: { $t: (key: string) => key },
      },
    });
    await new Promise((r) => setTimeout(r, 100));

    // Into the manager the way a reader gets there: "show all" under Recent.
    const showAll = wrapper.findAll('button').filter((b) => b.text() === 'note.show_all');
    await showAll[showAll.length - 1].trigger('click');
    await wrapper.vm.$nextTick();

    // One box in the header, one per row.
    const boxes = () => wrapper.findAll('input[type="checkbox"]');
    expect(boxes()).toHaveLength(4);

    // Nothing ticked yet, so no action bar.
    expect(wrapper.text()).not.toContain('note.selected_count');

    await boxes()[1].trigger('click');
    await wrapper.vm.$nextTick();
    expect(wrapper.text()).toContain('note.selected_count');

    // Shift reaches everything between, so all three rows end up ticked.
    await boxes()[3].trigger('click', { shiftKey: true });
    await wrapper.vm.$nextTick();

    const del = wrapper.findAll('button').find((b) => b.text().includes('note.delete_selected'));
    expect(del).toBeTruthy();
    await del!.trigger('click');
    await new Promise((r) => setTimeout(r, 50));

    // All three left the list, and — as with a single delete — nothing has
    // been moved on disk yet; the undo window is still open.
    expect((wrapper.vm as any).notes.length).toBe(0);
    expect(core.invoke).not.toHaveBeenCalledWith('trash_node_file', expect.anything());
  });
});
