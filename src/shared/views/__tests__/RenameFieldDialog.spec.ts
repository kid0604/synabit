import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import RenameFieldDialog from '../RenameFieldDialog.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        merge_title: 'Merge a field', rename_title: 'Rename a field',
        rename_pick_target: 'Rename it to what?', rename_new_name: 'A new name…',
        rename_apply: 'Rename', field_name: 'Field name',
        merge_because_taken: '“{field}” already exists on this kind, so this is a merge.',
        merge_will_change: '{count} nodes will be changed.',
        merge_will_skip: '{count} already have that field.',
        merge_apply: 'Merge', cancel: 'Cancel',
        merge_only_shape: 'No file carries “{field}” — only the shape changes.',
      },
    },
  },
});

const open = () =>
  mount(RenameFieldDialog, {
    props: {
      vaultPath: '/Users/x/vault',
      nodeType: 'animal',
      from: 'màu',
      candidates: ['colour', 'species'],
    },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });

const pick = async (key: string) => {
  const wrapper = open();
  const button = Array.from(document.body.querySelectorAll('button'))
    .find(b => b.textContent?.trim() === key);
  button?.click();
  await flushPromises();
  return wrapper;
};

describe('merging one field into another', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    invoke.mockReset();
    invoke.mockResolvedValue({ renaming: 2, skipped: 0, skipped_sample: [] });
  });

  /**
   * The whole point of the screen: nothing is written until the count has been
   * seen. The preview reads only.
   */
  it('asks what would happen before offering to do it', async () => {
    await pick('colour');

    expect(invoke).toHaveBeenCalledWith('preview_rename_property', {
      nodeType: 'animal',
      from: 'màu',
      to: 'colour',
    });
    expect(document.body.textContent).toContain('2 nodes will be changed.');
  });

  /**
   * The command writes files, so it takes a vault path — and it did not get
   * one. Tauri rejects a call with a missing argument before any Rust runs, so
   * merging failed every time with nothing to show for it but a log line.
   *
   * Neither type-check nor lint can see this: the payload is a plain object on
   * one side of a string-named command and a function signature on the other.
   */
  it('hands the backend the vault it is writing into', async () => {
    const wrapper = await pick('colour');
    invoke.mockClear();

    const apply = Array.from(document.body.querySelectorAll('button'))
      .find(b => b.textContent?.includes('Merge'));
    apply?.click();
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('rename_property', {
      vaultPath: '/Users/x/vault',
      nodeType: 'animal',
      from: 'màu',
      to: 'colour',
    });
    expect(wrapper.emitted('done')).toBeTruthy();
  });

  it('names the nodes it is going to leave alone', async () => {
    invoke.mockResolvedValue({
      renaming: 1,
      skipped: 1,
      skipped_sample: ['Animal/both.md'],
    });
    await pick('colour');

    expect(document.body.textContent).toContain('1 already have that field.');
    expect(document.body.textContent).toContain('Animal/both.md');
  });

  /**
   * A field declared and never filled in carries no files, and merging it is a
   * change to the shape and nothing else.
   *
   * The button used to grey out on a count of zero, which read as "this merge
   * is impossible" when it was simply cheap — and it made the one merge you
   * would want after designing a kind the one merge you could not do.
   */
  it('allows a merge that only changes the shape', async () => {
    invoke.mockResolvedValue({ renaming: 0, skipped: 0, skipped_sample: [] });
    await pick('colour');

    const apply = Array.from(document.body.querySelectorAll('button'))
      .find(b => b.textContent?.includes('Merge')) as HTMLButtonElement;

    expect(apply.disabled).toBe(false);
    expect(document.body.textContent, 'and says why the count is zero')
      .toContain('No file carries “màu”');
  });

  /**
   * Nothing is offered until the preview has come back.
   *
   * Found by position rather than by label, because the label is the thing
   * under test elsewhere: it reads "Rename" or "Merge" depending on whether
   * the destination is taken, and a test that looks for one word would pass or
   * fail on which case it happened to be in.
   */
  it('waits for the preview before offering the button', () => {
    open();
    const buttons = Array.from(document.body.querySelectorAll('button'));
    const apply = buttons[buttons.length - 1] as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
  });
});

/**
 * The destination travels with `done`, because the caller has a schema to
 * mend: a declared shape is a separate file and hears nothing about a rename,
 * so it would go on offering a key that no node carries any more.
 */
describe('what the dialog tells the caller afterwards', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    invoke.mockReset();
    invoke.mockResolvedValue({ renaming: 2, skipped: 0, skipped_sample: [] });
  });

  it('names the field it merged into', async () => {
    const wrapper = await pick('colour');
    const apply = Array.from(document.body.querySelectorAll('button'))
      .find(b => b.textContent?.includes('Merge'));
    apply?.click();
    await flushPromises();

    expect(wrapper.emitted('done')?.[0]).toEqual(['colour']);
  });
});

/**
 * Renaming and merging are one operation, and which it is depends on whether
 * the name is already taken.
 *
 * Only the existing keys were offered, which made the ordinary case — a field
 * called `due` that should have been `deadline` — the one thing that could not
 * be done, while the rarer case was the only one on the screen.
 */
describe('renaming to a name the kind has never had', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    invoke.mockReset();
    invoke.mockResolvedValue({ renaming: 4, skipped: 0, skipped_sample: [] });
  });

  const typeNewName = async (name: string) => {
    const wrapper = open();
    const start = Array.from(document.body.querySelectorAll('button'))
      .find(b => b.textContent?.includes('A new name'));
    start?.click();
    await flushPromises();

    const box = document.body.querySelector('input') as HTMLInputElement;
    box.value = name;
    box.dispatchEvent(new Event('input'));
    await flushPromises();
    return wrapper;
  };

  it('accepts a name typed by hand', async () => {
    await typeNewName('deadline');

    expect(invoke).toHaveBeenCalledWith('preview_rename_property', {
      nodeType: 'animal',
      from: 'màu',
      to: 'deadline',
    });
  });

  /** A fresh name is a rename; the button and the heading say so. */
  it('calls it a rename when the name is free', async () => {
    await typeNewName('deadline');

    expect(document.body.textContent).toContain('Rename a field');
    expect(document.body.textContent).not.toContain('already exists on this kind');
  });

  /** And the same screen calls it a merge when the name is taken. */
  it('calls it a merge when the name is already in use', async () => {
    await pick('colour');

    expect(document.body.textContent).toContain('Merge a field');
    expect(document.body.textContent).toContain('already exists on this kind');
  });
});
