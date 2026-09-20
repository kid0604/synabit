import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import MediaSurrogates from '../MediaSurrogates.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

const STATUS = {
  config: {
    transcripts: false,
    transcribe_url: '',
    transcribe_model: '',
    captions: false,
    caption_model: '',
  },
  desktop: true,
  running: false,
  local_provider: true,
  provider: 'ollama',
  pending_transcripts: 0,
  pending_captions: 0,
  done: 0,
  too_large: 0,
};

const open = async () => {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === 'timeline_media_status') return structuredClone(STATUS);
    return null;
  });
  const wrapper = mount(MediaSurrogates, {
    props: { vaultPath: '/vault' },
    global: { plugins: [i18n] },
  });
  await flushPromises();
  return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('The two settings at the bottom of the tray', () => {
  /// It sits under a long list of proposals, each of which is written the
  /// moment it is kept. A button that is live when there is nothing to save
  /// asks somebody whether their work is unsaved; an inert one answers.
  it('is inert until one of them is actually changed', async () => {
    const wrapper = await open();
    const save = wrapper.find('[data-media-save]');
    expect(save.attributes('disabled')).toBeDefined();

    await wrapper.findAll('input[type="checkbox"]')[0].setValue(true);
    expect(wrapper.find('[data-media-save]').attributes('disabled')).toBeUndefined();
  });

  it('saves only these settings, and nothing about the proposals', async () => {
    const wrapper = await open();
    await wrapper.findAll('input[type="checkbox"]')[0].setValue(true);
    await wrapper.find('[data-media-save]').trigger('click');
    await flushPromises();

    const sent = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_media_configure');
    expect(sent).toHaveLength(1);
    expect(sent[0][1]).toMatchObject({
      vaultPath: '/vault',
      settings: expect.objectContaining({ transcripts: true }),
    });
    expect(vi.mocked(invoke).mock.calls.map(c => c[0])).not.toContain('timeline_extract_review');
  });

  it('says what it saves', async () => {
    const wrapper = await open();
    expect(wrapper.find('[data-media-save]').text().toLowerCase()).toContain('settings');
  });
});
