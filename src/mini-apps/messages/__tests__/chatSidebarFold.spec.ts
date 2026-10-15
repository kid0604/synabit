import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { nextTick } from 'vue';
import { mount } from '@vue/test-utils';
import { i18n } from '../../../i18n';
import ChatSidebar from '../components/ChatSidebar.vue';

vi.mock('../../../shared/components/NavButtons.vue', () => ({ default: { render: () => null } }));

const chat = (id: string, title: string) => ({
  id,
  title,
  updated_at: '2026-10-14T00:00:00Z',
  message_count: 2,
  pinned: false,
});

const sidebar = (selected: string | null = null) =>
  mount(ChatSidebar, {
    props: {
      threads: [],
      conversations: [chat('a', 'Telegram'), chat('b', 'check Pi'), chat('c', 'tóm tắt bài này')] as never,
      selection: selected ? { kind: 'conversation', id: selected } : null,
      unread: 0,
      online: true,
      stats: null,
      footing: null,
    },
    global: { plugins: [i18n] },
  });

const titles = (w: ReturnType<typeof sidebar>) =>
  w.findAll('.group span.block.text-\\[13px\\]').map((s) => s.text());

const header = (w: ReturnType<typeof sidebar>) =>
  w.findAll('button[aria-expanded]').find((b) => b.text().includes('Conversations'))!;

const fakeStorage = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  } as unknown as Storage;
};

beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()));
afterEach(() => vi.unstubAllGlobals());

describe('folding the conversations', () => {
  it('folds them away, keeps the open one, and remembers', async () => {
    const w = sidebar('b');
    expect(titles(w)).toHaveLength(3);

    await header(w).trigger('click');
    expect(header(w).attributes('aria-expanded')).toBe('false');
    expect(titles(w)).toEqual(['check Pi']);
    await nextTick();
    expect(localStorage.getItem('syn.conversations.list')).toBe('folded');

    // Still folded the next time, with nothing open.
    expect(titles(sidebar())).toEqual([]);
  });

  it('unfolds for a search', async () => {
    localStorage.setItem('syn.conversations.list', 'folded');
    const w = sidebar();
    expect(titles(w)).toEqual([]);

    await w.find('input[type="text"]').setValue('tóm');
    expect(titles(w)).toEqual(['tóm tắt bài này']);
  });
});
