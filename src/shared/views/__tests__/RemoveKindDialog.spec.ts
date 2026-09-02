import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import RemoveKindDialog from '../RemoveKindDialog.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        remove_kind_title: 'Remove {type}?',
        remove_kind_why: 'none | {type} is on one file. | {type} is on {n} files.',
        remove_kind_empty: 'Nothing carries {type}.',
        remove_kind_retype: 'Move to another kind',
        remove_kind_retype_why: 'none | The file keeps everything. | All {n} keep everything.',
        remove_kind_delete: 'none | Delete the file | Delete all {n} files',
        remove_kind_delete_why: 'It goes to the trash. | They go to the trash.',
        rename_new_name: 'A new name…', remove_kind_move_apply: 'Move',
        kind_name_hint: 'book…', delete: 'Delete', cancel: 'Cancel',
      },
    },
  },
});

const open = async (declared = false) => {
  const wrapper = mount(RemoveKindDialog, {
    props: { vaultPath: '/vault', nodeType: 'abc', declared, candidates: ['note', 'book'] },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
};

const click = async (label: string) => {
  Array.from(document.body.querySelectorAll('button'))
    .find(b => b.textContent?.includes(label))
    ?.click();
  await flushPromises();
};

const submit = () => {
  const buttons = Array.from(document.body.querySelectorAll('button'));
  return buttons[buttons.length - 1] as HTMLButtonElement;
};

describe('taking a kind out of the vault', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    invoke.mockReset();
    invoke.mockResolvedValue({ nodes: 3 });
  });

  /**
   * The fork is the design. Changing the word and removing the files are
   * opposite in consequence — one keeps everything, one ends it — and no
   * button can work out which somebody meant.
   */
  it('offers both ways out and commits to neither', async () => {
    await open();

    expect(document.body.textContent).toContain('Move to another kind');
    expect(document.body.textContent).toContain('Delete all 3 files');

    // The safe outcome is chosen for you; the button still waits on a
    // destination, so nothing is one careless click from happening.
    expect(submit().textContent, 'and never names a delete nobody chose')
      .toContain('Move');
    expect(submit().disabled).toBe(true);
  });

  it('retypes every node without deleting one', async () => {
    const wrapper = await open();
    await click('Move to another kind');
    await click('note');
    invoke.mockClear();

    submit().click();
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('retype_kind', {
      vaultPath: '/vault',
      fromType: 'abc',
      toType: 'note',
    });
    expect(wrapper.emitted('done')).toBeTruthy();
  });

  it('deletes the nodes when that is what was chosen', async () => {
    await open();
    await click('Delete all 3 files');
    invoke.mockClear();

    submit().click();
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('delete_kind', {
      vaultPath: '/vault',
      nodeType: 'abc',
    });
  });

  /** A kind nothing carries has no fork: there is only a declaration. */
  it('asks nothing when no file carries the kind', async () => {
    invoke.mockResolvedValue({ nodes: 0 });
    await open(true);

    expect(document.body.textContent).toContain('Nothing carries abc.');
    expect(document.body.textContent).not.toContain('Move to another kind');
    expect(submit().disabled, 'there is one outcome, so it is ready').toBe(false);
  });

  /** And that case must not reach the bulk delete: there is nothing to delete. */
  it('runs no deletion for a kind with no files', async () => {
    invoke.mockResolvedValue({ nodes: 0 });
    const wrapper = await open(true);
    invoke.mockClear();

    submit().click();
    await flushPromises();

    expect(invoke).not.toHaveBeenCalled();
    expect(wrapper.emitted('done')).toBeTruthy();
  });

  /**
   * A count of one used to read "1 files say", "All 1 keep everything they
   * hold" and "Delete all 1". Vietnamese has no plural, so writing the strings
   * in it hid the problem until somebody read the English.
   */
  it('reads properly when the kind is on a single file', async () => {
    invoke.mockResolvedValue({ nodes: 1 });
    await open();
    const text = document.body.textContent ?? '';

    expect(text).toContain('abc is on one file.');
    expect(text).toContain('The file keeps everything.');
    expect(text).toContain('Delete the file');
    expect(text).not.toContain('1 files');
    expect(text).not.toContain('all 1');
  });
});
