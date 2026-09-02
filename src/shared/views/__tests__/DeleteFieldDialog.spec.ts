import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import DeleteFieldDialog from '../DeleteFieldDialog.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        delete_field_title: 'Delete “{field}” from every {type}?',
        delete_field_count: '{count} nodes lose this field and the value in it. Each node’s version history still holds what was there.',
        delete_field_none: 'No node carries “{field}” — only the shape changes.',
        delete: 'Delete', cancel: 'Cancel',
      },
    },
  },
});

const open = async () => {
  const wrapper = mount(DeleteFieldDialog, {
    props: { vaultPath: '/vault', nodeType: 'animal', field: 'màu' },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
};

const button = (label: string) =>
  Array.from(document.body.querySelectorAll('button'))
    .find(b => b.textContent?.includes(label)) as HTMLButtonElement;

describe('ending a field on every node of a kind', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    invoke.mockReset();
    invoke.mockResolvedValue({ deleting: 2 });
  });

  /** The count comes first; the button is not offered until it is on screen. */
  it('counts the files before offering to do anything', async () => {
    await open();

    expect(invoke).toHaveBeenCalledWith('preview_delete_property', {
      nodeType: 'animal',
      key: 'màu',
    });
    expect(document.body.textContent).toContain('2 nodes lose this field');
  });

  /**
   * Not an undo, and not unrecoverable either. Both halves are said, in the
   * same sentence as the count rather than in a paragraph of their own.
   */
  it('says the values are still in each node’s history', async () => {
    await open();
    expect(document.body.textContent).toContain('version history still holds');
  });

  /**
   * Nothing appears until the count is in. A confirmation whose numbers arrive
   * after it does is one somebody has already started reading.
   */
  it('shows nothing at all until it knows the count', () => {
    mount(DeleteFieldDialog, {
      props: { vaultPath: '/vault', nodeType: 'animal', field: 'màu' },
      global: { plugins: [i18n] },
      attachTo: document.body,
    });

    expect(document.body.textContent).not.toContain('Delete');
  });

  it('hands the backend the vault it is writing into', async () => {
    await open();
    invoke.mockClear();

    button('Delete').click();
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('delete_property', {
      vaultPath: '/vault',
      nodeType: 'animal',
      key: 'màu',
    });
  });

  /**
   * A field declared and never filled in. Nothing to lose, and still worth
   * doing: it comes out of the shape.
   */
  it('allows a deletion that only changes the shape', async () => {
    invoke.mockResolvedValue({ deleting: 0 });
    await open();

    expect(document.body.textContent).toContain('No node carries “màu”');
    // Nothing is at risk, so it is not dressed as a destructive act.
    expect(document.body.textContent).not.toContain('version history still holds');
    expect(button('Delete')).toBeTruthy();
  });
});
