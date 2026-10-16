import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => []), convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('../../../../utils/logger', () => ({ logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() } }));
vi.mock('../../../../composables/useNodeService', () => ({ useNodeService: () => ({ writeNode: vi.fn() }) }));

import MessageBubble from '../MessageBubble.vue';

/**
 * "Đã lưu vào QuickCap." with nothing called — three from Telegram, and
 * nothing kept. The bubble says so under the words.
 */
let wrapper: VueWrapper<any> | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

const bubble = async (content: string, tools: { tool_name: string; result_preview: string }[] = []) => {
  wrapper = mount(MessageBubble, {
    props: { message: { id: 'm', role: 'assistant', content, timestamp: '', tool_calls_log: tools.map((t) => ({ conversation_id: 'c', tool_args: {}, iteration: 0, ...t })) } as any },
    global: { plugins: [i18n] },
  });
  await flushPromises();
  return wrapper;
};
const warned = (w: VueWrapper<any>) => w.find('[role="note"]').exists();

describe('an answer that says it did something', () => {
  it('is marked when nothing ran', async () => {
    expect(warned(await bubble('Đã lưu vào QuickCap.'))).toBe(true);
  });

  it('is not when a tool went through', async () => {
    expect(warned(await bubble('Đã lưu vào QuickCap.', [{ tool_name: 'capture', result_preview: '{"kept":true}' }]))).toBe(false);
  });

  it('is when the only call was refused', async () => {
    expect(warned(await bubble('Đã lưu vào QuickCap.', [{ tool_name: 'capture', result_preview: '{"refused":"no"}' }]))).toBe(true);
  });

  it('is not when it claims nothing', async () => {
    expect(warned(await bubble('Pi đang 57,5°C.'))).toBe(false);
  });
});
