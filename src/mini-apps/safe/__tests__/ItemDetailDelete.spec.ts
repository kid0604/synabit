import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { setActivePinia, createPinia } from 'pinia';
import ItemDetail from '../ItemDetail.vue';
import type { SafeApi, ItemView } from '../api';
import { useAppStore } from '../../../stores/useAppStore';
import { pendingDeleteQuestion, answerDeleteQuestion } from '../../../composables/useConfirmDelete';

vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));

const item = {
  id: 'abc', kind: 'login', title: 'Bank', fields: [], urls: [], tags: [], favorite: false, notes: '', links: [],
  totp: null, ai_level: 'hidden', handle: null, ai_destinations: [], health: [], expires_at: null, created_at: 1, updated_at: 1, history_count: 0, trashed_at: null,
} as unknown as ItemView;

function fakeApi() {
  const calls: [string, unknown[]][] = [];
  const api = new Proxy({} as SafeApi, {
    get: (_, name) => typeof name !== 'string' || name.startsWith('__') || name === 'then' ? undefined : async (...args: unknown[]) => {
      calls.push([name, args]);
      return undefined;
    },
  });
  return { api, calls };
}

const mountIt = () => {
  const { api, calls } = fakeApi();
  const wrapper = mount(ItemDetail, {
    props: { api, item },
    global: { stubs: { SynAccess: { props: ['api', 'item'], template: '<div />' }, ConfirmModal: true } },
  });
  return { wrapper, calls };
};

/**
 * Moving an item into the Safe's trash is an ordinary delete — Restore undoes
 * it — so only "Ask before deleting" asks. Purge keeps its own question.
 */
describe('trashing a Safe item', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    pendingDeleteQuestion.value = null;
  });

  it('asks nothing when "Ask before deleting" is off', async () => {
    const { wrapper, calls } = mountIt();
    await wrapper.find('[aria-label="safe.detail.trash"]').trigger('click');
    await flushPromises();
    expect(pendingDeleteQuestion.value).toBeNull();
    expect(calls).toContainEqual(['setTrashed', ['abc', true]]);
  });

  it('asks first when it is on, and leaves the item alone on no', async () => {
    useAppStore().confirmBeforeDelete = true;
    const { wrapper, calls } = mountIt();
    await wrapper.find('[aria-label="safe.detail.trash"]').trigger('click');
    expect(pendingDeleteQuestion.value?.name).toBe('Bank');
    answerDeleteQuestion(false);
    await flushPromises();
    expect(calls.some(([name]) => name === 'setTrashed')).toBe(false);
  });
});
