import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';

/**
 * Enter in the quick-entry box while an input method is composing.
 *
 * With Telex or VNI, Enter commits the word being built. The box saved on it,
 * so a capture typed in Vietnamese was cut off mid-word (review §5, U7).
 */

const { invoke, hide } = vi.hoisted(() => ({
  invoke: vi.fn(async (_command: string, _args?: unknown) => undefined),
  hide: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ hide, onFocusChanged: async () => () => {} }),
}));
vi.mock('@tauri-apps/plugin-store', () => ({
  load: async () => ({ get: async () => undefined }),
}));
vi.mock('../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import QuickEntry from '../QuickEntry.vue';

let wrapper: VueWrapper<any> | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

const press = async (init: KeyboardEventInit & { keyCode?: number }) => {
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  // jsdom's constructor ignores `keyCode`; WebKit sends 229 during composition.
  if (init.keyCode !== undefined) Object.defineProperty(event, 'keyCode', { value: init.keyCode });
  wrapper!.find('textarea').element.dispatchEvent(event);
  await flushPromises();
};

const captures = () => invoke.mock.calls.filter(([c]) => c === 'queue_capture');

describe('the quick-entry box', () => {
  it('leaves Enter to the input method while it is composing', async () => {
    wrapper = mount(QuickEntry, { attachTo: document.body });
    await flushPromises();
    await wrapper.find('textarea').setValue('mua suwx');

    await press({ key: 'Enter', isComposing: true });
    await press({ key: 'Enter', keyCode: 229 });
    expect(captures()).toHaveLength(0);

    await press({ key: 'Enter' });
    expect(captures()).toHaveLength(1);
  });
});
