import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DatedView from '../DatedView.vue';
import { i18n } from '../../../i18n';

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
