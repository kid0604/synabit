import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import EventsOverTime from '../EventsOverTime.vue';
import { i18n } from '../../../i18n';
import type { QueryResult } from '../types';

const answer = (days: string[], total = days.length): QueryResult => ({
  columns: ['when', 'title'],
  rows: days.map((day, i) => ({ id: `e${i}`, node_type: 'note', title: `Chuyện ${i}`, cells: [day, `Chuyện ${i}`], open: `Notes/${i}.md` })),
  total,
  query_time_ms: 1,
});

// Over 400 days, so the grain is `month` (`grainFor`): Jan 2025 → Jun 2026 is
// eighteen buckets, three of them busy — Jan 2025, Jun 2025, Jun 2026.
const YEAR = ['2025-01-05', '2025-01-20', '2025-06-02', '2025-06-10', '2025-06-28', '2026-06-15'];

const mountChart = (result: QueryResult | null, modelValue = null as null | { from: string; to: string }) =>
  mount(EventsOverTime, { props: { result, modelValue }, global: { plugins: [i18n] } });

/**
 * A pointer event at `x` px. Built by hand: jsdom has no `PointerEvent`, and
 * `trigger` cannot set `clientX` on the `MouseEvent` it falls back to. The
 * handler reads nothing a `MouseEvent` does not have.
 */
const fire = async (wrapper: ReturnType<typeof mountChart>, type: string, x: number) => {
  wrapper.find('[data-over-time-hit]').element.dispatchEvent(new MouseEvent(type, { clientX: x, bubbles: true }));
  await wrapper.vm.$nextTick();
};

/** A press on the plot at `x` px, with no drag. */
const pressAt = async (wrapper: ReturnType<typeof mountChart>, x: number) => {
  await fire(wrapper, 'pointerdown', x);
  await fire(wrapper, 'pointerup', x);
};

describe('Events drawn across time', () => {
  it('draws a column for each busy stretch and a dot for each event', () => {
    const wrapper = mountChart(answer(YEAR));
    // Three busy months; the fifteen quiet ones keep their place on the axis
    // and paint nothing.
    const painted = wrapper.findAll('[data-over-time-bar]').filter(bar => bar.attributes('d'));
    expect(painted).toHaveLength(3);
    expect(wrapper.findAll('[data-over-time-dot]')).toHaveLength(YEAR.length);
  });

  /// A one-bar chart is a number pretending to be a picture.
  it('says it in a sentence when everything is in one stretch', () => {
    const wrapper = mountChart(answer(['2026-03-02', '2026-03-02']));
    expect(wrapper.find('[data-over-time-bar]').exists()).toBe(false);
    expect(wrapper.find('[data-over-time-one]').text()).toContain('2');
    // The dots stay: each one is still something that can be opened.
    expect(wrapper.findAll('[data-over-time-dot]')).toHaveLength(2);
  });

  it('has nothing to draw when nothing has a day', () => {
    const wrapper = mountChart(answer([]));
    expect(wrapper.find('[data-over-time-empty]').exists()).toBe(true);
  });

  /// A chart of a page, drawn as if it were everything, is the worst kind
  /// of wrong: nobody can tell.
  it('says when it is drawing only part of the answer', () => {
    const wrapper = mountChart(answer(YEAR, 1340));
    expect(wrapper.find('[data-over-time-note]').text()).toContain('1340');
  });
});

describe('Picking a stretch of time', () => {
  /// The plot runs from x=36 to x=628 in the default 640px box, over
  /// eighteen months; January 2025 is the first ~33px of it.
  it('picks the stretch under a press', async () => {
    const wrapper = mountChart(answer(YEAR));
    await pressAt(wrapper, 50);
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([{ from: '2025-01-01', to: '2025-01-31' }]);
  });

  it('lets go when the same stretch is pressed again', async () => {
    const wrapper = mountChart(answer(YEAR), { from: '2025-01-01', to: '2025-01-31' });
    await pressAt(wrapper, 50);
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([null]);
  });

  /// Dragged across, snapped out to whole months.
  it('picks everything a drag crosses', async () => {
    const wrapper = mountChart(answer(YEAR));
    await fire(wrapper, 'pointerdown', 50);
    await fire(wrapper, 'pointermove', 250);
    await fire(wrapper, 'pointerup', 250);
    const [range] = wrapper.emitted('update:modelValue')![0] as [{ from: string; to: string }];
    expect(range.from).toBe('2025-01-01');
    // x=250 lands in the middle of 2025; snapped out to the end of its month.
    expect(range.to > '2025-06-01' && range.to < '2025-09-01').toBe(true);
    const end = new Date(`${range.to}T00:00:00`);
    end.setDate(end.getDate() + 1);
    expect(end.getDate()).toBe(1);
  });

  it('says how many are in the stretch, and can let it go', async () => {
    const wrapper = mountChart(answer(YEAR), { from: '2025-06-01', to: '2025-06-30' });
    expect(wrapper.find('[data-over-time-selected]').text()).toContain('3');
    await wrapper.find('[data-over-time-clear]').trigger('click');
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([null]);
  });

  /// What the pointer does, the keyboard does.
  it('can be driven from the keyboard', async () => {
    const wrapper = mountChart(answer(YEAR));
    const hit = wrapper.find('[data-over-time-hit]');
    await hit.trigger('keydown', { key: 'ArrowRight' });
    await hit.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('update:modelValue')?.[0]).toEqual([{ from: '2025-01-01', to: '2025-01-31' }]);
    await hit.trigger('keydown', { key: 'Escape' });
    expect(wrapper.emitted('update:modelValue')?.[1]).toEqual([null]);
  });
});

describe('The events themselves', () => {
  it('opens the one that was pressed', async () => {
    const wrapper = mountChart(answer(YEAR));
    await wrapper.findAll('[data-over-time-dot]')[2].trigger('click');
    expect(wrapper.emitted('open')?.[0][0]).toMatchObject({ id: 'e2', open: 'Notes/2.md' });
  });

  /// A title is data from a note: shown as text, never read as markup.
  it('shows a title as text on hover', async () => {
    const result = answer(YEAR);
    result.rows[0].title = '<img src=x onerror=alert(1)>';
    const wrapper = mountChart(result);
    await wrapper.findAll('[data-over-time-dot]')[0].trigger('pointerenter');
    const tip = wrapper.find('[data-over-time-tooltip]');
    expect(tip.text()).toContain('<img');
    expect(tip.find('img').exists()).toBe(false);
  });
});

describe('The axis', () => {
  /// One day long, and d3's own ticks came every three hours — nine labels
  /// all reading «Jul 8». Labels come from the buckets now.
  it('never labels finer than the chart counts', () => {
    const wrapper = mountChart(answer(['2026-07-08']));
    const labels = wrapper.findAll('[data-over-time-tick]').map(t => t.text());
    expect(labels).toHaveLength(1);
  });

  it('never repeats a label', () => {
    const wrapper = mountChart(answer(YEAR));
    const labels = wrapper.findAll('[data-over-time-tick]').map(t => t.text());
    expect(new Set(labels).size).toBe(labels.length);
    expect(labels.length).toBeGreaterThan(1);
  });
});

describe('A stretch of time', () => {
  /// Four years at university added to every column it crossed would put a 1
  /// on forty-eight months, and the columns would stop meaning anything.
  it('is a bar of its own above the columns, and is not counted in them', () => {
    const result = answer([...YEAR, '2025-02-01']);
    result.rows[result.rows.length - 1].until = '2026-03-31';
    result.rows[result.rows.length - 1].title = 'Làm ở MDP';
    const wrapper = mountChart(result);

    const lanes = wrapper.findAll('[data-over-time-span]');
    expect(lanes).toHaveLength(1);
    expect(lanes[0].text()).toContain('Làm ở MDP');
    // Six days and one stretch: the stretch is not a dot either.
    expect(wrapper.findAll('[data-over-time-dot]')).toHaveLength(6);
    // And the columns count what they counted before it was there.
    const counted = (w: ReturnType<typeof mountChart>) =>
      w.findAll('[data-over-time-bar]').map(bar => Number(bar.attributes('data-count')));
    expect(counted(wrapper)).toEqual(counted(mountChart(answer(YEAR))));
    expect(counted(wrapper).reduce((sum, n) => sum + n, 0)).toBe(6);
  });

  it('says so when it runs past the edge of the chart', () => {
    const result = answer(['2025-01-05', '2025-06-02', '2026-06-15']);
    result.rows[0].until = '2030-12-31';
    const wrapper = mountChart(result);
    expect(wrapper.find('[data-over-time-span]').text()).toContain('→');
  });
});
