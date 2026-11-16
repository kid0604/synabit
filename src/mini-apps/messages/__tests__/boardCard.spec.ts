import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { i18n } from '../../../i18n';

/**
 * The board under an answer Syn drew it in: its buttons open it beside the
 * conversation and in the Whiteboard.
 *
 * Run rather than read: the first version's button did nothing because of
 * the order of a click handler's branches, each correct on its own. The
 * picture is its own component now (`BoardPicture`), stubbed here — it needs
 * a layout engine jsdom does not have — and what is checked is that the
 * bubble finds the board, hands it over, and passes on what it asks for.
 */

const PATH = 'Whiteboards/whiteboard-1794586303594.whiteboard.json';

const { invoke } = vi.hoisted(() => ({
  invoke: vi.fn(async (command: string): Promise<unknown> => {
    if (command === 'read_whiteboard') {
      return JSON.stringify({
        schemaVersion: 1,
        title: 'Lưu đồ quy trình Change Request',
        nodes: [
          { id: 'a', type: 'shape', position: { x: 0, y: 0 }, data: { label: 'Bắt đầu', width: 120, height: 56 } },
          { id: 'b', type: 'shape', position: { x: 0, y: 120 }, data: { label: 'Kết thúc', width: 120, height: 56 } },
        ],
        edges: [{ id: 'e', source: 'a', target: 'b' }],
      });
    }
    return null;
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke, convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));

describe('a board under an answer', () => {
  it('is drawn, and opens beside the conversation or in the Whiteboard app', async () => {
    const { default: MessageBubble } = await import('../components/MessageBubble.vue');
    const wrapper = mount(MessageBubble, {
      attachTo: document.body,
      props: {
        vaultPath: '/vault',
        message: {
          id: 'm1',
          role: 'assistant',
          content: 'Đã vẽ xong.',
          timestamp: '2026-11-13T16:11:44Z',
          tool_calls_log: [
            { tool_name: 'draw_board', result_preview: JSON.stringify({ board: PATH, title: 'Lưu đồ' }) },
          ],
        } as any,
      },
      global: {
        plugins: [i18n, createPinia()],
        stubs: { BoardPicture: true, SynVisual: true, DiagramViewer: true, KeepAsLens: true, HowWorkedOut: true, FootingMark: true, PlanList: true },
      },
    });

    const { default: BoardPicture } = await import('../components/BoardPicture.vue');
    let picture = wrapper.findComponent(BoardPicture);
    for (let i = 0; i < 20 && !picture.exists(); i++) {
      await flushPromises();
      picture = wrapper.findComponent(BoardPicture);
    }
    expect(picture.exists(), 'the board was drawn under the answer').toBe(true);
    expect(picture.props('board')).toMatchObject({ path: PATH, title: 'Lưu đồ quy trình Change Request' });

    picture.vm.$emit('open');
    picture.vm.$emit('arrange');
    await flushPromises();

    expect(wrapper.emitted('open-source')).toEqual([[{ id: PATH, title: 'Lưu đồ quy trình Change Request', node_type: 'whiteboard' }]]);
    expect(wrapper.emitted('open-board')?.[0]?.[0]).toMatchObject({ path: PATH });
    wrapper.unmount();
  });
});
