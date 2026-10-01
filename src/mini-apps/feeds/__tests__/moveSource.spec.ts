import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import FeedsSidebar from '../components/FeedsSidebar.vue';
import FeedSourceItem from '../components/FeedSourceItem.vue';
import { FEED_DRAG_TYPE } from '../dragType';
import { i18n } from '../../../i18n';

const categories = [
  { id: 'kb', name: 'KB', color: '#f97316' },
  { id: 'ai', name: 'AI', color: '#ec4899' },
] as any[];
const sources = [
  { id: 'tia', title: 'Tia Sáng', categoryId: 'kb', url: 'u', feedType: 'rss', isPaused: false, fullTextFetch: false, scrapeContainer: '' },
] as any[];

const mountSidebar = () => mount(FeedsSidebar, {
  props: {
    sources, categories, unreadCounts: {}, viewCounts: { today: 0, unread: 0, starred: 0, readLater: 0, all: 0 },
    selectedSourceId: null, selectedCategoryId: null, currentView: 'all',
  } as any,
  global: { plugins: [i18n], stubs: { 'lucide-vue-next': true } },
});

/** jsdom has no DataTransfer; this is the part a drop reads. */
const transfer = (id: string) => ({
  types: [FEED_DRAG_TYPE],
  getData: (type: string) => (type === FEED_DRAG_TYPE ? id : ''),
  setData: () => {},
  dropEffect: '',
  effectAllowed: '',
});

const zones = (w: ReturnType<typeof mountSidebar>) => w.findAll('[class*="space-y-0.5"][class*="rounded-xl"]');

describe('moving a feed to another category', () => {
  it('moves it when it is dropped on a category', async () => {
    const w = mountSidebar();
    const ai = zones(w)[1];
    await ai.trigger('dragover', { dataTransfer: transfer('tia') });
    await ai.trigger('drop', { dataTransfer: transfer('tia') });
    expect(w.emitted('move-source')).toEqual([['tia', 'ai']]);
  });

  it('ignores drags that are not feeds, like files from the desktop', async () => {
    const w = mountSidebar();
    await zones(w)[1].trigger('drop', { dataTransfer: { types: ['Files'], getData: () => '' } });
    expect(w.emitted('move-source')).toBeUndefined();
  });

  it('offers the same move in the feed menu, without dragging', async () => {
    const w = mountSidebar();
    const item = w.findComponent(FeedSourceItem);
    await item.find(`button[aria-label="${i18n.global.t('feeds.a11y_open_menu')}"]`).trigger('click');
    const toAi = item.findAll('button').find(b => b.text() === 'AI');
    expect(toAi).toBeTruthy();
    await toAi!.trigger('click');
    expect(w.emitted('move-source')).toEqual([['tia', 'ai']]);
  });
});
