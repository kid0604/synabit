import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import SafeMain from '../SafeMain.vue';
import type { SafeApi } from '../api';

vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));

const item = {
  id: 'abc', kind: 'login', title: 'Bank', fields: [], urls: [], tags: [], favorite: false, notes: '', links: [],
  totp: null, ai_level: 'hidden', handle: null, ai_destinations: [], health: [], expires_at: null, created_at: 1, updated_at: 1, history_count: 0, trashed_at: null,
};

function fakeApi() {
  const calls: [string, unknown[]][] = [];
  const api = new Proxy({} as SafeApi, {
    // Only method names: Vue probes props for symbols and `__v_*` flags, and a
    // function there is taken for something it is not.
    get: (_, name) => typeof name !== 'string' || name.startsWith('__') || name === 'then' ? undefined : async (...args: unknown[]) => {
      calls.push([name, args]);
      if (name === 'overview') return { all: 1, favorites: 0, trash: 0, kinds: [], tags: [], unreadable: [], unhealthy: 0, health: [], breach_checked_at: null };
      if (name === 'getSettings') return { auto_lock_secs: 600, clipboard_clear_secs: 30, breach_check: false };
      if (name === 'list') return [];
      if (name === 'get') return item;
      return undefined;
    },
  });
  return { api, calls };
}

describe('the open Safe', () => {
  /**
   * A `synabit://safe/<id>` link in a note arrives as `openId`. The watch that
   * answers it once ran above a variable it touched and threw where nothing
   * reported it — the item simply never opened.
   */
  it('opens the item a link asked for', async () => {
    const { api, calls } = fakeApi();
    const wrapper = mount(SafeMain, {
      props: { api, openId: 'abc' },
      global: { stubs: { NavButtons: true, ItemDetail: { props: ['item', 'api'], template: '<h2>{{ item.title }}</h2>' } } },
    });
    await flushPromises();
    expect(calls.some(([name, args]) => name === 'get' && args[0] === 'abc')).toBe(true);
    expect(wrapper.find('h2').text()).toBe('Bank');
    expect(wrapper.emitted('opened')).toBeTruthy();
  });
});
