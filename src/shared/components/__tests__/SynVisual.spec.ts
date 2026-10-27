import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { ref } from 'vue';
import { createI18n } from 'vue-i18n';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: vi.fn() }));
vi.mock('@tauri-apps/plugin-fs', () => ({ writeTextFile: vi.fn() }));
vi.mock('../../mermaid', () => ({ diagramTheme: ref('dark') }));
vi.mock('../../../composables/usePlatform', () => ({ usePlatform: () => ({ isMobileOS: ref(false), isSmallScreen: ref(false) }) }));
vi.mock('../../../composables/useNodeService', () => ({ useNodeService: () => ({ createNode: vi.fn(), writeNode: vi.fn() }) }));
vi.mock('../../../utils/logger', () => ({ logger: { error: vi.fn(), info: vi.fn(), warn: vi.fn() } }));

import SynVisual from '../SynVisual.vue';

const ID = '0123456789abcdef0123456789abcdef';
const i18n = createI18n({ legacy: false, locale: 'vi', missingWarn: false, fallbackWarn: false, messages: { vi: {} } });

const mountIt = async () => {
  const wrapper = mount(SynVisual, { props: { visualId: ID, vaultPath: '/vault' }, global: { plugins: [i18n] }, attachTo: document.body });
  await flushPromises();
  return wrapper;
};

beforeEach(() => {
  invoke.mockReset();
  invoke.mockResolvedValue({ id: ID, title: 'Con lắc đơn', kind: 'simulation', question: 'q' });
});

describe('a visual in the chat', () => {
  it('is framed sandboxed to scripts alone, on its own origin', async () => {
    const wrapper = await mountIt();
    const frame = wrapper.get('iframe');
    expect(frame.attributes('sandbox')).toBe('allow-scripts');
    expect(frame.attributes('src')).toBe(`synvisual://localhost/v/${ID}?theme=dark`);
    expect(invoke).toHaveBeenCalledWith('syn_visual_meta', { vaultPath: '/vault', id: ID });
    expect(wrapper.text()).toContain('Con lắc đơn');
    wrapper.unmount();
  });

  it('listens to its own frame and to nothing else', async () => {
    const wrapper = await mountIt();
    const frame = wrapper.get('iframe').element as HTMLIFrameElement;
    const before = frame.style.height;

    window.dispatchEvent(new MessageEvent('message', { data: { jsonrpc: '2.0', method: 'ui/notifications/size-changed', params: { height: 777 } }, source: window }));
    await flushPromises();
    expect(frame.style.height).toBe(before);

    window.dispatchEvent(new MessageEvent('message', { data: { jsonrpc: '2.0', method: 'ui/notifications/size-changed', params: { height: 777 } }, source: frame.contentWindow }));
    await flushPromises();
    expect(frame.style.height).toBe('777px');

    window.dispatchEvent(new MessageEvent('message', { data: { jsonrpc: '2.0', method: 'ui/notifications/size-changed', params: { height: 99999 } }, source: frame.contentWindow }));
    await flushPromises();
    expect(frame.style.height).toBe('900px');
    wrapper.unmount();
  });

  it('passes a question on, and never sends it', async () => {
    const wrapper = await mountIt();
    const frame = wrapper.get('iframe').element as HTMLIFrameElement;
    window.dispatchEvent(new MessageEvent('message', {
      data: { jsonrpc: '2.0', id: 3, method: 'ui/message', params: { content: { type: 'text', text: 'Gấp đôi thì sao?' } } },
      source: frame.contentWindow,
    }));
    await flushPromises();
    expect(wrapper.emitted('ask')).toEqual([['Gấp đôi thì sao?']]);
    expect(invoke).not.toHaveBeenCalledWith('syn_send_message', expect.anything());
    wrapper.unmount();
  });

  it('says so when the visual is gone', async () => {
    invoke.mockRejectedValue(new Error('There is no visual'));
    const wrapper = await mountIt();
    expect(wrapper.find('iframe').exists()).toBe(false);
    expect(wrapper.text()).toContain('syn.visual_missing');
    wrapper.unmount();
  });
});
