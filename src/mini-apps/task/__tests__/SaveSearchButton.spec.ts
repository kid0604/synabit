import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import SaveSearchButton from '../components/SaveSearchButton.vue';

const stubs = { global: { mocks: { $t: (key: string) => key } } };
const make = (suggestedName = 'due_date:<2026-09-01') =>
  mount(SaveSearchButton, { props: { suggestedName }, ...stubs });

/**
 * The first version asked with `window.prompt`, which returns null without
 * showing anything in this app's WebView — no dialog, no file, no error. These
 * pin that the name is asked for in the page, where nothing can be missing.
 */
describe('naming a search in place', () => {
  it('shows a button, not a field, to begin with', () => {
    const w = make();
    expect(w.find('input').exists()).toBe(false);
    expect(w.find('button').exists()).toBe(true);
  });

  it('opens a field when pressed', async () => {
    const w = make();
    await w.find('button').trigger('click');
    expect(w.find('input').exists()).toBe(true);
  });

  /** The common case should be press, press Enter. */
  it('seeds the field with the query', async () => {
    const w = make('#work overdue');
    await w.find('button').trigger('click');
    expect((w.find('input').element as HTMLInputElement).value).toBe('#work overdue');
  });

  it('cuts a very long query down to something typeable', async () => {
    const w = make('x'.repeat(200));
    await w.find('button').trigger('click');
    expect((w.find('input').element as HTMLInputElement).value.length).toBe(40);
  });

  it('saves on Enter', async () => {
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('Overdue at work');
    await w.find('input').trigger('keydown.enter');
    expect(w.emitted('save')).toEqual([['Overdue at work']]);
  });

  it('trims the name', async () => {
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('  Spaced  ');
    await w.find('input').trigger('keydown.enter');
    expect(w.emitted('save')).toEqual([['Spaced']]);
  });

  it('saves nothing for an empty name', async () => {
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('   ');
    await w.find('input').trigger('keydown.enter');
    expect(w.emitted('save')).toBeUndefined();
  });

  it('goes back to a button after saving', async () => {
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('Kept');
    await w.find('input').trigger('keydown.enter');
    expect(w.find('input').exists()).toBe(false);
  });

  it('cancels on Escape without saving', async () => {
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('Never mind');
    await w.find('input').trigger('keydown.escape');
    expect(w.emitted('save')).toBeUndefined();
    expect(w.find('input').exists()).toBe(false);
  });

  it('forgets what was typed when reopened', async () => {
    const w = make('query');
    await w.find('button').trigger('click');
    await w.find('input').setValue('abandoned');
    await w.find('input').trigger('keydown.escape');
    await w.find('button').trigger('click');
    expect((w.find('input').element as HTMLInputElement).value).toBe('query');
  });

  it('never calls window.prompt', async () => {
    const prompt = vi.spyOn(window, 'prompt').mockReturnValue(null);
    const w = make();
    await w.find('button').trigger('click');
    await w.find('input').setValue('Named');
    await w.find('input').trigger('keydown.enter');
    expect(prompt).not.toHaveBeenCalled();
    prompt.mockRestore();
  });
});
