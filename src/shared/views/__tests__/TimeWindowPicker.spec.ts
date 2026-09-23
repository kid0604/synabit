import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TimeWindowPicker from '../TimeWindowPicker.vue';
import { i18n } from '../../../i18n';
import type { WindowChoice } from '../overTime';

const mountPicker = (modelValue: WindowChoice = '1y', hidden = 0) =>
  mount(TimeWindowPicker, {
    props: { modelValue, range: { from: '2025-09-23', to: '2026-09-22' }, hidden },
    global: { plugins: [i18n] },
  });

describe('Choosing a stretch of time', () => {
  it('marks the one in use and picks another in one press', async () => {
    const wrapper = mountPicker('1y');
    expect(wrapper.find('[data-preset="1y"]').attributes('aria-pressed')).toBe('true');
    await wrapper.find('[data-preset="90d"]').trigger('click');
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['90d']);
  });

  /// Said, with the way out one press away.
  it('says what it leaves out and offers all of it', async () => {
    const wrapper = mountPicker('1y', 7);
    expect(wrapper.find('[data-window-hidden]').text()).toContain('7');
    await wrapper.find('[data-window-all]').trigger('click');
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual(['all']);
  });

  it('says nothing when nothing is left out', () => {
    expect(mountPicker('all', 0).find('[data-window-hidden]').exists()).toBe(false);
  });

  /// Starts from what is on screen, and applies once both ends are days.
  it('takes a stretch of one’s own', async () => {
    const wrapper = mountPicker('1y');
    await wrapper.find('[data-window-custom]').trigger('click');
    expect((wrapper.find('[data-window-from]').element as HTMLInputElement).value).toBe('2025-09-23');
    await wrapper.find('[data-window-from]').setValue('2024-01-01');
    await wrapper.find('[data-window-from]').trigger('change');
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([{ from: '2024-01-01', to: '2026-09-22' }]);
  });

  /// A WebView that draws this as a text box lets anything be typed.
  it('refuses a stretch that is not two days running forwards', async () => {
    const wrapper = mountPicker('1y');
    await wrapper.find('[data-window-custom]').trigger('click');
    await wrapper.find('[data-window-from]').setValue('2027-01-01');
    await wrapper.find('[data-window-from]').trigger('change');
    expect(wrapper.find('[data-window-backwards]').exists()).toBe(true);
    await wrapper.find('[data-window-from]').setValue('22/09/2026');
    await wrapper.find('[data-window-from]').trigger('change');
    expect(wrapper.emitted('update:modelValue')).toBeUndefined();
  });
});
