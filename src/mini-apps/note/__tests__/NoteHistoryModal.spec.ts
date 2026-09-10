import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import NoteHistoryModal from '../NoteHistoryModal.vue';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn().mockResolvedValue(true) }));
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } }),
}));
vi.mock('../../../utils/logger', () => ({ logger: { error: vi.fn(), warn: vi.fn() } }));

const versions = [
  { id: '1:9', timestamp: 2, size: 20, delta: 5, is_current: true, is_local: true },
  { id: '1:3', timestamp: 1, size: 15, delta: 15, is_current: false, is_local: true },
];

let order: string[];

function mountModal(beforeRestore?: () => Promise<void>) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === 'list_node_versions') return versions;
    if (cmd === 'diff_node_version') return { groups: [], added: 0, removed: 0, unchanged: true };
    if (cmd === 'restore_node_version') { order.push('restore'); return undefined; }
    return undefined;
  });
  return mount(NoteHistoryModal, {
    props: { vaultPath: '/v', noteId: 'Notes/n.md', noteTitle: 'N', beforeRestore },
    global: { stubs: { 'lucide-vue-next': true }, mocks: { $t: (key: string) => key } },
  });
}

async function restoreOldest(wrapper: ReturnType<typeof mountModal>) {
  await flushPromises();
  // The version list: one left-aligned button per version, newest first.
  const rows = wrapper.findAll('button.text-left');
  expect(rows).toHaveLength(versions.length);
  await rows[rows.length - 1].trigger('click');
  await flushPromises();
  const restore = wrapper.findAll('button').find((b) => b.text() === 'note.history_restore');
  await restore!.trigger('click');
  await flushPromises();
}

describe('NoteHistoryModal restore', () => {
  beforeEach(() => { vi.clearAllMocks(); order = []; });

  // Words still waiting on the autosave are in no version. The restore keeps
  // the version it replaces, so that version has to be on disk first.
  it('saves what the editor holds before restoring over it', async () => {
    const beforeRestore = vi.fn(async () => { order.push('save'); });
    const wrapper = mountModal(beforeRestore);
    await restoreOldest(wrapper);

    expect(order).toEqual(['save', 'restore']);
    expect(invoke).toHaveBeenCalledWith('restore_node_version', {
      vaultPath: '/v', relPath: 'Notes/n.md', versionId: '1:3',
    });
  });

  // The restored text is a whole file, frontmatter included; the parent reads
  // the note back from disk instead. Nothing rides on the event to misuse.
  it('says a restore happened without handing over the file', async () => {
    const wrapper = mountModal(async () => {});
    await restoreOldest(wrapper);

    expect(wrapper.emitted('restored')).toEqual([[]]);
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('does not restore over work it could not save', async () => {
    const wrapper = mountModal(async () => { throw new Error('disk full'); });
    await restoreOldest(wrapper);

    expect(order).toEqual([]);
    expect(wrapper.emitted('restored')).toBeUndefined();
  });
});
