import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import * as core from '@tauri-apps/api/core';
import SyncConflictsDialog from '../SyncConflictsDialog.vue';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key }),
}));

const invoke = core.invoke as unknown as ReturnType<typeof vi.fn>;

const CAP = 'QuickCaps/d438.md';
const COPY_1 = 'QuickCaps/d438 (conflict 63a42537).md';
const COPY_2 = 'QuickCaps/d438 (conflict 63a42537) (conflict 63a42537).md';
const NOTE_COPY = 'Notes/plan (conflict 11111111).md';
const GONE = 'Notes/old (conflict 22222222).md';

const groups = [
  {
    original: CAP,
    title: 'Ý tưởng: chụp ảnh lavabo',
    node_type: 'quickcap',
    copies: [
      { rel_path: COPY_1, difference: 'none' },
      { rel_path: COPY_2, difference: 'none' },
    ],
  },
  {
    original: 'Notes/plan.md',
    title: 'Plan',
    node_type: 'note',
    copies: [{ rel_path: NOTE_COPY, difference: 'text' }],
  },
];

const mounted: VueWrapper[] = [];
const open = async () => {
  const wrapper = mount(SyncConflictsDialog, {
    props: {
      show: true,
      vaultPath: '/vault',
      conflicts: [COPY_1, COPY_2, NOTE_COPY, GONE].map((kept_as) => ({ rel_path: '', kept_as })),
    },
    global: { mocks: { $t: (key: string, ...rest: unknown[]) => `${key}${rest.length ? JSON.stringify(rest[0]) : ''}` } },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  await flushPromises();
  return wrapper;
};

const buttons = () => Array.from(document.body.querySelectorAll('button'));
const button = (text: string) => buttons().find((b) => b.textContent?.includes(text))!;

beforeEach(() => {
  document.body.innerHTML = '';
  invoke.mockReset();
  invoke.mockImplementation((cmd: string, args: { copies?: string[] }) => {
    if (cmd === 'describe_kept_copies') return Promise.resolve(structuredClone(groups));
    if (cmd === 'trash_identical_kept_copies') return Promise.resolve(args.copies);
    return Promise.reject(new Error(cmd));
  });
});

afterEach(() => {
  while (mounted.length) mounted.pop()!.unmount();
});

describe('SyncConflictsDialog', () => {
  it('names each note once, by its title, rather than listing paths', async () => {
    await open();
    const text = document.body.textContent ?? '';
    expect(text).toContain('Ý tưởng: chụp ảnh lavabo');
    expect(text).toContain('Plan');
    expect(text).not.toContain('(conflict');
    expect(text).toContain('shell.conflicts.identical');
    expect(text).toContain('shell.conflicts.differs_text');
  });

  it('forgets a copy that is no longer on disk as soon as it looks', async () => {
    const wrapper = await open();
    expect(wrapper.emitted('forget')?.[0]).toEqual([[GONE]]);
  });

  it('trashes only the identical copies, never one that differs', async () => {
    const wrapper = await open();
    button('shell.conflicts.trash_all').click();
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('trash_identical_kept_copies', {
      vaultPath: '/vault',
      copies: [COPY_1, COPY_2],
    });
    expect(wrapper.emitted('forget')?.[1]).toEqual([[COPY_1, COPY_2]]);
    // The note whose text differs is still there, and still not offered for deletion.
    expect(document.body.textContent).toContain('Plan');
    expect(button('shell.conflicts.trash_all')).toBeUndefined();
    expect(wrapper.emitted('close')).toBeUndefined();
  });

  it('opens the differing copy in the app it belongs to', async () => {
    const wrapper = await open();
    button('shell.conflicts.open_copy').click();
    expect(wrapper.emitted('open')?.[0]).toEqual([NOTE_COPY, 'note']);
  });

  it('closes once nothing is left to show', async () => {
    invoke.mockImplementation((cmd: string, args: { copies?: string[] }) => {
      if (cmd === 'describe_kept_copies') return Promise.resolve([structuredClone(groups[0])]);
      return Promise.resolve(args.copies);
    });
    const wrapper = await open();
    button('shell.conflicts.trash_all').click();
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
  });
});
