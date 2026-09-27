import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

/**
 * What the model writes cannot style the screen (review §5, S9).
 *
 * An answer that could style the page could lay a block over the permission
 * card's buttons, or relabel them. Its prose gets no `<style>` and no `style`.
 */

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
  convertFileSrc: (p: string) => p,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));
vi.mock('../../../../composables/useNodeService', () => ({
  useNodeService: () => ({ writeNode: vi.fn() }),
}));

import MessageBubble from '../MessageBubble.vue';

let wrapper: VueWrapper<any> | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

describe('what the model writes', () => {
  it('cannot bring its own styles', async () => {
    wrapper = mount(MessageBubble, {
      props: {
        message: {
          id: 'm',
          role: 'assistant',
          content:
            'Hello <style>.x{}</style><span style="position:fixed;inset:0">over the card</span>',
          timestamp: '',
        },
      },
      global: { plugins: [i18n] },
    });
    await flushPromises();
    const html = wrapper.html();
    expect(html).toContain('over the card');
    expect(html).not.toContain('<style');
    expect(html).not.toContain('position:fixed');
    expect(wrapper.find('span[style]').exists()).toBe(false);
  });
});
