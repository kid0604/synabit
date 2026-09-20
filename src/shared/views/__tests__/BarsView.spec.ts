import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import BarsView from '../BarsView.vue';
import { i18n } from '../../../i18n';
import type { QueryResult } from '../types';

const tally = (rows: [string, string][]): QueryResult => ({
  columns: ['month', 'count'],
  rows: rows.map(([label, value]) => ({
    id: label,
    node_type: '',
    title: label,
    cells: [label, value],
  })),
  total: rows.length,
  query_time_ms: 1,
});

const draw = (result: QueryResult | null) =>
  mount(BarsView, { props: { result }, global: { plugins: [i18n] } });

const widths = (wrapper: ReturnType<typeof draw>) =>
  wrapper.findAll('[data-bar-fill]').map(b => b.attributes('style') ?? '');

describe('An answer drawn as bars', () => {
  /// Against the largest, not against the total: the question is which of
  /// these is bigger, and a share-of-total scale flattens everything once
  /// there are twenty of them.
  it('scales every bar against the biggest one', () => {
    const wrapper = draw(tally([['2019-11', '4'], ['2021-03', '2'], ['2022-01', '1']]));
    expect(widths(wrapper)).toEqual([
      expect.stringContaining('width: 100%'),
      expect.stringContaining('width: 50%'),
      expect.stringContaining('width: 25%'),
    ]);
  });

  it('shows the number beside the bar, because the bar is the comparison', () => {
    const wrapper = draw(tally([['2019-11', '4']]));
    expect(wrapper.find('[data-bar-label]').text()).toBe('2019-11');
    expect(wrapper.find('[data-bar-value]').text()).toBe('4');
  });

  /// A row that is there but empty is still a row, so it keeps a hairline
  /// rather than disappearing into the track.
  it('still draws a row whose number is nothing', () => {
    const wrapper = draw(tally([['2019-11', '4'], ['2021-03', '0']]));
    expect(wrapper.findAll('[data-bar]')).toHaveLength(2);
    expect(widths(wrapper)[1]).toContain('width: 1.5%');
  });

  it('says so plainly when there is nothing to draw', () => {
    expect(draw(tally([])).find('[data-bars-empty]').exists()).toBe(true);
  });

  /// A heap of rows is not a node, so there is nothing to open.
  it('offers nothing to click, because a heap is not a thing', () => {
    const wrapper = draw(tally([['2019-11', '4']]));
    expect(wrapper.findAll('button')).toHaveLength(0);
  });
});
