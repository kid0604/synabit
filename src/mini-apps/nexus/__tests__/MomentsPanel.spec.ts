import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import MomentsPanel, { type MediaEntry, type MediaMoment } from '../components/MomentsPanel.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => `asset://${path}` }));

const entry = (id: string, overrides: Partial<MediaEntry> = {}): MediaEntry => ({
  node_id: `Files/${id}.md`,
  title: `${id}.jpg`,
  extension: 'jpg',
  kind: 'image',
  at: '2024-09-12 10:00',
  present: true,
  path: `/vault/assets/${id}.jpg`,
  caption: null,
  caption_model: null,
  transcript: null,
  ...overrides,
});

const mountPanel = async (moments: MediaMoment[]) => {
  vi.mocked(invoke).mockResolvedValue(moments);
  const wrapper = mount(MomentsPanel, { props: { vaultPath: '/vault', atDate: '2024-09-12' }, global: { plugins: [i18n] } });
  await flushPromises();
  await wrapper.find('button').trigger('click');
  return wrapper;
};

describe('MomentsPanel', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('asks for the month the strip is at', async () => {
    await mountPanel([]);
    expect(invoke).toHaveBeenCalledWith('timeline_media_moments', { vaultPath: '/vault', when: '2024-09' });
  });

  it('shows what stands in for a picture whose original is on another device', async () => {
    const away = entry('beach', { present: false, path: null, caption: 'Two people on a beach at dusk.', caption_model: 'llava' });
    const wrapper = await mountPanel([{ from: away.at, to: away.at, count: 1, cover: away, members: [away] }]);
    expect(wrapper.find('img').exists()).toBe(false);
    expect(wrapper.find('[data-elsewhere]').exists()).toBe(true);
    expect(wrapper.find('[data-caption]').text()).toContain('Two people on a beach at dusk.');
    expect(wrapper.find('[data-caption]').text()).toContain('llava');
  });

  it('opens a recording at the line that was clicked', async () => {
    const memo = entry('memo', {
      kind: 'audio',
      extension: 'm4a',
      title: 'memo.m4a',
      transcript: { text: 'about the contract', model: 'whisper-small', segments: [{ start: 0, end: 4.2, text: 'Today' }, { start: 192, end: 230.4, text: 'about the contract' }] },
    });
    const wrapper = await mountPanel([{ from: memo.at, to: memo.at, count: 1, cover: memo, members: [memo] }]);

    const lines = wrapper.findAll('[data-segment]');
    expect(lines[1].text()).toContain('3:12');
    await lines[1].trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual(['Files/memo.md', 'file', 't=192,230.4']);
  });
});
