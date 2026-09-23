import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DatedView from '../DatedView.vue';
import { i18n } from '../../../i18n';
import type { QueryResult } from '../types';

describe('A long title in a narrow column', () => {
  /// Narrowing the column clipped titles at its edge: the row would not
  /// shrink below its content, so the ellipsis never came and the end of the
  /// sentence simply vanished.
  it('lets the row shrink and the title wrap rather than be cut', () => {
    const wrapper = mount(DatedView, {
      props: {
        result: {
          columns: ['when', 'title'],
          rows: [{ id: 'a', node_type: 'note', title: 'NGFW khởi động lại gây lỗi kết nối từ ứng dụng đến cơ sở dữ liệu', cells: ['2026-09-14', 'x'] }],
          total: 1,
          query_time_ms: 1,
        },
      },
      global: { plugins: [i18n] },
    });
    expect(wrapper.find('[data-dated-row]').classes()).toContain('min-w-0');
    const title = wrapper.find('[data-dated-title]');
    expect(title.classes()).toContain('break-words');
    expect(title.classes()).not.toContain('truncate');
    expect(title.text()).toContain('cơ sở dữ liệu');
  });
});

/** An answer of `[day, title]` rows. */
const answer = (rows: [string, string][]): QueryResult => ({
  columns: ['when', 'title'],
  rows: rows.map(([day, title], i) => ({ id: `e${i}`, node_type: 'moment', title, cells: [day, title] })),
  total: rows.length,
  query_time_ms: 1,
});

describe('A stretch of time in the list', () => {
  it('says where it ends, under the day it began', () => {
    const result = answer([['2019-01-01', 'Làm ở MDP']]);
    result.rows[0].until = '2021-06-30';
    const wrapper = mount(DatedView, { props: { result }, global: { plugins: [i18n] } });
    expect(wrapper.find('[data-dated-until]').text()).toContain('2021-06-30');
  });

  /// A job of three years belongs under the day it began, which may be years
  /// above the top of a window it filled. "Nothing happened" would be the
  /// wrong reading of that window.
  it('is pinned at the top when it was already going on', () => {
    const result = answer([['2019-01-01', 'Làm ở MDP'], ['2026-03-02', 'Ăn trưa với Nga']]);
    result.rows[0].until = '2026-12-31';
    const wrapper = mount(DatedView, {
      props: { result, range: { from: '2026-01-01', to: '2026-12-31' } },
      global: { plugins: [i18n] },
    });
    const pinned = wrapper.find('[data-ongoing]');
    expect(pinned.text()).toContain('Làm ở MDP');
    expect(pinned.text()).toContain('2019-01-01 → 2026-12-31');
    // And it is not listed twice.
    expect(wrapper.findAll('[data-dated-row]')).toHaveLength(2);
    expect(wrapper.find('[data-day]').text()).toContain('Ăn trưa với Nga');
  });
});
