import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import DeleteButton from '../components/DeleteButton.vue';

const stubs = { global: { mocks: { $t: (key: string) => key } } };

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/**
 * Two presses of one button, the second on a control that has visibly changed.
 * It is a real confirmation — a stray click cannot delete — without a modal
 * taking the focus and covering the task being deleted.
 */
describe('inline confirmation', () => {
  it('does not delete on the first press', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    expect(w.emitted('confirm')).toBeUndefined();
  });

  it('says what the second press will do', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    expect(w.text()).toContain('task.delete_confirm');
  });

  it('deletes on the second press', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    await w.trigger('click');
    expect(w.emitted('confirm')).toHaveLength(1);
  });

  /** A button left saying "Delete?" is a trap for the next click near it. */
  it('goes back to a bin on its own', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    await vi.advanceTimersByTimeAsync(3500);
    expect(w.text()).not.toContain('task.delete_confirm');
  });

  it('needs two presses again after it times out', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    await vi.advanceTimersByTimeAsync(3500);
    await w.trigger('click');
    expect(w.emitted('confirm')).toBeUndefined();
  });

  it('disarms when the pointer leaves', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    await w.trigger('click');
    await w.trigger('mouseleave');
    await w.trigger('click');
    expect(w.emitted('confirm')).toBeUndefined();
  });

  /** A screen reader must be told the button has changed what it does. */
  it('renames itself when armed', async () => {
    const w = mount(DeleteButton, { props: { mode: 'inline' }, ...stubs });
    expect(w.attributes('aria-label')).toBe('task.a11y_delete_task');
    await w.trigger('click');
    expect(w.attributes('aria-label')).toBe('task.delete_confirm');
  });
});

describe('the other two settings', () => {
  /** The dialog is opened upstream, so the button itself fires straight away. */
  it('fires on the first press in dialog mode', async () => {
    const w = mount(DeleteButton, { props: { mode: 'dialog' }, ...stubs });
    await w.trigger('click');
    expect(w.emitted('confirm')).toHaveLength(1);
  });

  it('fires on the first press in undo mode', async () => {
    const w = mount(DeleteButton, { props: { mode: 'undo' }, ...stubs });
    await w.trigger('click');
    expect(w.emitted('confirm')).toHaveLength(1);
  });

  it('never arms outside inline mode', async () => {
    const w = mount(DeleteButton, { props: { mode: 'undo' }, ...stubs });
    await w.trigger('click');
    expect(w.text()).not.toContain('task.delete_confirm');
  });

  /** The default has to be the safe one, for any caller that forgets to pass it. */
  it('arms by default when no mode is given', async () => {
    const w = mount(DeleteButton, { ...stubs });
    await w.trigger('click');
    expect(w.emitted('confirm')).toBeUndefined();
  });
});
