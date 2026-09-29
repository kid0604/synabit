import { describe, it, expect, vi, beforeAll } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import ItemEditor from '../ItemEditor.vue';
import type { ItemEdit, ItemView, SafeApi } from '../api';

vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string) => key, locale: { value: 'en' } }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

beforeAll(() => {
  // jsdom has `<dialog>` but not the modal half of it.
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});

const github: ItemView = {
  id: 'aa', kind: 'login', title: 'GitHub',
  fields: [
    { id: 'u', label: 'Username', kind: 'username', concealed: false, value: 'anh', length_bucket: null, empty: false },
    { id: 'p', label: 'Password', kind: 'password', concealed: true, value: null, length_bucket: 16, empty: false },
  ],
  urls: [{ url: 'https://github.com', match: 'domain' }], tags: ['work'], favorite: true, notes: '', links: [],
  totp: { algorithm: 'sha1', digits: 6, period: 30 }, handle: null, ai_destinations: [], health: [],
  ai_level: 'hidden', expires_at: null, created_at: 1, updated_at: 2, history_count: 0, trashed_at: null,
};

/** A fake API that records every call, so the test can say what was *not* asked. */
function fakeApi() {
  const calls: string[] = [];
  const saved: ItemEdit[] = [];
  const api = new Proxy({} as SafeApi, {
    get: (_, name: string) => async (...args: unknown[]) => {
      calls.push(name);
      if (name === 'updateItem' || name === 'createItem') {
        saved.push(args[args.length - 1] as ItemEdit);
        return github;
      }
      if (name === 'generate') return { value: 'Generated-Value-123', bits: 100 };
      return undefined;
    },
  });
  return { api, calls, saved };
}

describe('the item editor', () => {
  /**
   * The property the whole editor is built around: editing an item does not
   * bring its passwords into the WebView. An untouched concealed field goes
   * back as `unchanged`, and nothing asks Rust for its value.
   */
  it('saves an untouched password as unchanged, without ever revealing it', async () => {
    const { api, calls, saved } = fakeApi();
    const wrapper = mount(ItemEditor, { props: { api, item: github, kind: 'login' } });
    await flushPromises();

    expect(wrapper.html()).not.toContain('hunter');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(calls).not.toContain('reveal');
    expect(calls).not.toContain('get');
    expect(saved).toHaveLength(1);
    const password = saved[0].fields.find((f) => f.id === 'p');
    expect(password?.value).toEqual({ t: 'unchanged' });
    const username = saved[0].fields.find((f) => f.id === 'u');
    expect(username?.value).toEqual({ t: 'set', v: 'anh' });
    expect(saved[0].favorite).toBe(true);
    expect(saved[0].totp, 'an existing one-time-code key was sent back').toEqual({ t: 'unchanged' });
  });

  it('removes a one-time code only when asked', async () => {
    const { api, saved } = fakeApi();
    const wrapper = mount(ItemEditor, { props: { api, item: github, kind: 'login' } });
    await flushPromises();
    const remove = wrapper.findAll('button').find((b) => b.text() === 'safe.totp.remove');
    await remove!.trigger('click');
    await wrapper.find('form').trigger('submit');
    await flushPromises();
    expect(saved[0].totp).toEqual({ t: 'remove' });
  });

  it('sends a new value only once the user chose to change it', async () => {
    const { api, saved } = fakeApi();
    const wrapper = mount(ItemEditor, { props: { api, item: github, kind: 'login' } });
    await flushPromises();

    const change = wrapper.findAll('button').find((b) => b.text() === 'safe.editor.change');
    expect(change).toBeTruthy();
    await change!.trigger('click');
    const input = wrapper.findAll('input[type="password"]');
    expect(input).toHaveLength(1);
    await input[0].setValue('correct horse battery');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(saved[0].fields.find((f) => f.id === 'p')?.value).toEqual({ t: 'set', v: 'correct horse battery' });
  });

  it('starts a new item from its kind’s template', async () => {
    const { api, saved } = fakeApi();
    const wrapper = mount(ItemEditor, { props: { api, item: null, kind: 'login' } });
    await flushPromises();
    await wrapper.find('input').setValue('Bank');
    await wrapper.find('form').trigger('submit');
    await flushPromises();

    expect(saved[0].title).toBe('Bank');
    expect(saved[0].fields.map((f) => f.kind)).toEqual(['username', 'password']);
    expect(saved[0].fields.every((f) => f.id === null)).toBe(true);
  });
});
