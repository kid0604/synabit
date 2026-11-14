import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia } from 'pinia';
import { i18n } from '../../../i18n';

/**
 * The board under an answer Syn drew it in: its button opens the Whiteboard.
 *
 * Run rather than read, because the defect was in the order of the click
 * handler's branches, not in any one of them: the button is a `data-act`, and
 * the branch that sends every `data-act` to `runBlockAction` came before the
 * one that looked for this button. Each branch read correctly on its own and
 * the button did nothing.
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
  it('opens in the Whiteboard app from its button', async () => {
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
        stubs: { SynVisual: true, DiagramViewer: true, KeepAsLens: true, HowWorkedOut: true, FootingMark: true, PlanList: true },
      },
    });

    let button: HTMLElement | null = null;
    for (let i = 0; i < 20 && !button; i++) {
      await flushPromises();
      button = wrapper.element.querySelector(`[data-act="board-app"][data-for="${PATH}"]`);
    }
    expect(button, 'the board card was drawn under the answer').not.toBeNull();

    button!.click();
    await flushPromises();

    expect(wrapper.emitted('open-source')).toEqual([[{ id: PATH, title: 'Lưu đồ quy trình Change Request', node_type: 'whiteboard' }]]);
    wrapper.unmount();
  });
});
