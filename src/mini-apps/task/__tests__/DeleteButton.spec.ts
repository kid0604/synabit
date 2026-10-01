import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DeleteButton from '../components/DeleteButton.vue';

const stubs = { global: { mocks: { $t: (key: string) => key } } };

/**
 * One press, like every delete in the app. Whether to ask first is the
 * app-wide setting's job, upstream of this button.
 */
describe('DeleteButton', () => {
  it('deletes on the first press', async () => {
    const w = mount(DeleteButton, stubs);
    await w.trigger('click');
    expect(w.emitted('confirm')).toHaveLength(1);
  });

  it('never turns into a second "Delete" button', async () => {
    const w = mount(DeleteButton, stubs);
    await w.trigger('click');
    expect(w.text()).not.toContain('task.delete_confirm');
  });

  it('is named for screen readers', () => {
    const w = mount(DeleteButton, stubs);
    expect(w.attributes('aria-label')).toBe('task.a11y_delete_task');
    expect(w.attributes('title')).toBe('task.a11y_delete_task');
  });

  it('is tighter when compact', () => {
    expect(mount(DeleteButton, { props: { compact: true }, ...stubs }).classes()).toContain('p-0.5');
  });
});
