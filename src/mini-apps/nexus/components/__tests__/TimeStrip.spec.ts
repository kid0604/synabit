import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TimeStrip from '../TimeStrip.vue';
import { i18n } from '../../../../i18n';
import type { TimeFrame } from '../../timeFrame';

const frame = (earliest: string | null): TimeFrame => ({
  first_seen: {},
  died_on: {},
  links: [],
  density: earliest ? [{ month: earliest, count: 3, weight: 6 }, { month: '2016-05', count: 1, weight: 2 }] : [],
  earliest,
});

const pad = (n: number) => String(n).padStart(2, '0');
const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

const monthsBetween = (first: string) => {
  const [y0, m0] = first.split('-').map(Number);
  const d = new Date();
  return (d.getFullYear() - y0) * 12 + (d.getMonth() + 1 - m0) + 1;
};

const mountStrip = (props: { frame: TimeFrame | null; modelValue: string | null }) =>
  mount(TimeStrip, { props, global: { plugins: [i18n] } });

const lastEmitted = (wrapper: ReturnType<typeof mountStrip>) =>
  wrapper.emitted('update:modelValue')?.slice(-1)[0]?.[0];

describe('TimeStrip', () => {
  it('starts at the present, which is today rather than the end of this month', () => {
    const wrapper = mountStrip({ frame: frame('2016-01'), modelValue: null });
    expect(lastEmitted(wrapper)).toBe(today());
    wrapper.unmount();
  });

  it('draws one mark for every month since the first dated thing', () => {
    const wrapper = mountStrip({ frame: frame('2016-01'), modelValue: null });
    expect(wrapper.findAll('rect')).toHaveLength(monthsBetween('2016-01'));
    wrapper.unmount();
  });

  it('moves a month with an arrow key and a year with Shift', async () => {
    const wrapper = mountStrip({ frame: frame('2016-01'), modelValue: '2016-05-31' });
    const slider = wrapper.find('[role="slider"]');

    await slider.trigger('keydown', { key: 'ArrowLeft' });
    expect(lastEmitted(wrapper)).toBe('2016-04-30');

    await slider.trigger('keydown', { key: 'ArrowRight', shiftKey: true });
    expect(lastEmitted(wrapper)).toBe('2017-04-30');

    await slider.trigger('keydown', { key: 'Home' });
    expect(lastEmitted(wrapper)).toBe('2016-01-31');
    wrapper.unmount();
  });

  it('says so when nothing in the vault has a date', () => {
    const wrapper = mountStrip({ frame: frame(null), modelValue: null });
    expect(wrapper.find('[role="slider"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('Nothing in this vault has a date yet');
    wrapper.unmount();
  });

  it('hatches a sealed period over its months', () => {
    const sealedFrame: TimeFrame = {
      ...frame('2016-01'),
      sealed: [{ id: 's1', from: '2016-02-01', to: '2016-04-30', from_text: '2016-02', to_text: '2016-04' }],
    };
    const wrapper = mountStrip({ frame: sealedFrame, modelValue: null });
    expect(wrapper.findAll('[data-sealed-band]')).toHaveLength(1);
    wrapper.unmount();
  });

  it('seals a period only when both ends are a month or a year, in order', async () => {
    const wrapper = mountStrip({ frame: frame('2016-01'), modelValue: null });
    await wrapper.find('button[aria-expanded]').trigger('click');

    const [from, to] = wrapper.findAll('input[type="text"]');
    const submit = () => wrapper.findAll('button')
      .find(b => b.text() === 'Seal' && b.attributes('aria-expanded') === undefined)!;

    await from.setValue('2019-09');
    await to.setValue('2019-02');
    expect(submit().attributes('disabled')).toBeDefined();

    await to.setValue('2019-12');
    await submit().trigger('click');
    expect(wrapper.emitted('seal-period')?.[0]).toEqual(['2019-09', '2019-12']);
    wrapper.unmount();
  });

  it('goes back to the present when closed', async () => {
    const wrapper = mountStrip({ frame: frame('2016-01'), modelValue: '2016-05-31' });
    const back = wrapper.findAll('button').find(b => b.text().includes('Back to now'))!;
    await back.trigger('click');
    expect(wrapper.emitted('close')).toHaveLength(1);
    wrapper.unmount();
  });
});
