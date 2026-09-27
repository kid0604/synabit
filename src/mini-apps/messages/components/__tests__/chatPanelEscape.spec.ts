import { describe, it, expect, vi, afterEach, beforeAll } from 'vitest';
import { shallowMount, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

/**
 * Escape in the conversation panel (review §5, U3).
 *
 * The panel listened on `window` and is kept alive behind `<keep-alive>`, so
 * closing a dialog in another app stopped a run in this one.
 */

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
  convertFileSrc: (p: string) => p,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn(async () => null) }));
vi.mock('@tauri-apps/plugin-fs', () => ({ readFile: vi.fn(async () => new Uint8Array()) }));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import ChatPanel from '../ChatPanel.vue';

const mounted: VueWrapper<any>[] = [];
afterEach(() => {
  mounted.splice(0).forEach((w) => w.unmount());
});

// jsdom lays nothing out, so it has no `scrollTo` for the panel to call.
beforeAll(() => {
  if (!Element.prototype.scrollTo) Element.prototype.scrollTo = () => {};
});

const panel = () => {
  const wrapper = shallowMount(ChatPanel, {
    props: { messages: [], streamingContent: '', isStreaming: true },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  return wrapper;
};

describe('Escape in the conversation panel', () => {
  it('does not stop the run for an Escape pressed elsewhere', () => {
    const wrapper = panel();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(wrapper.emitted('stop')).toBeUndefined();
  });

  it('stops it for an Escape pressed inside, once, and says it answered', () => {
    const wrapper = panel();
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    wrapper.element.dispatchEvent(event);
    expect(wrapper.emitted('stop')).toHaveLength(1);
    expect(event.defaultPrevented, 'so the app around it does not stop it again').toBe(true);
  });

  it('leaves an Escape that something inside already answered', () => {
    const wrapper = panel();
    const event = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true });
    event.preventDefault();
    wrapper.element.dispatchEvent(event);
    expect(wrapper.emitted('stop')).toBeUndefined();
  });
});
