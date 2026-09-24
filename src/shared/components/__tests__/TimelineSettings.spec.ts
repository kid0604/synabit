import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import TimelineSettings from '../TimelineSettings.vue';
import type { ExtractStatus } from '../../timelineReading';
import { i18n } from '../../../i18n';
import { useEventBus } from '../../../composables/useEventBus';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const status = (overrides: Partial<ExtractStatus> = {}): ExtractStatus => ({
  config: { enabled: false, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
  syn_enabled: true,
  provider: 'ollama',
  local: true,
  model: 'gemma',
  desktop: true,
  running: false,
  unreadable: [],
  proposals: [],
  people: [],
  categories: ['meal', 'spending', 'meeting', 'work', 'health', 'trip', 'family', 'feeling', 'thought', 'milestone', 'other'],
  moments: {},
  ...overrides,
});

const on = (overrides: Partial<ExtractStatus> = {}) =>
  status({ config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] }, ...overrides });

const show = async (initial: ExtractStatus) => {
  vi.mocked(invoke).mockImplementation(async (command: string) => (command === 'timeline_extract_status' ? initial : null));
  const wrapper = mount(TimelineSettings, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
  await flushPromises();
  return wrapper;
};

describe('TimelineSettings', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('does not turn on a cloud provider until sending notes there is allowed', async () => {
    const wrapper = await show(status({ local: false, provider: 'gemini' }));
    expect(wrapper.find('[data-cloud-warning]').text()).toContain('gemini');
    expect(wrapper.find('[data-enable]').attributes('disabled')).toBeDefined();

    await wrapper.find('[data-allow-cloud]').setValue(true);
    await wrapper.find('[data-enable]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('timeline_extract_configure', expect.objectContaining({
      settings: expect.objectContaining({ enabled: true, allow_cloud: true }),
    }));
  });

  /// Two questions with two prices. What reading is set up to do is rows the
  /// timeline already holds; how much is left to read means walking every note
  /// in the vault and replaying its history. Asked as one, opening this tab
  /// showed nothing at all for a second — so the screen draws on the first
  /// answer and fills the numbers in when the second arrives.
  it('shows the settings before it knows how much is left to read', async () => {
    let arrive: (left: unknown) => void = () => {};
    const later = new Promise(resolve => { arrive = resolve; });
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'timeline_extract_status') return on({ categories: ['meal', 'other'] });
      if (command === 'timeline_reading_left') return later;
      return null;
    });
    const wrapper = mount(TimelineSettings, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
    await flushPromises();

    // Everything else is already here, and usable: reading is a button, not
    // a number, and it does not wait on one.
    expect(wrapper.find('[data-kinds]').findAll('[data-kind]')).toHaveLength(2);
    expect(wrapper.find('[data-reset-ask]').exists()).toBe(true);
    expect(wrapper.find('[data-run-new]').attributes('disabled')).toBeUndefined();
    expect(wrapper.find('[data-left-looking]').exists()).toBe(true);

    arrive({ pending: 12, old_version: 0, done: 4, estimate_ms: 180_000, estimate_measured: false });
    await flushPromises();
    expect(wrapper.find('[data-left-looking]').exists()).toBe(false);
    expect(wrapper.text()).toContain('12');
  });

  it('never reads anything just by being opened', async () => {
    await show(on());
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).not.toContain('timeline_extract_run');
  });

  /// The list of kinds is a list, and this is where it is kept — whether or
  /// not reading is on, because it is the same list either way.
  it('shows the kinds and can drop one, with reading on or off', async () => {
    for (const showing of [status({ categories: ['meal', 'work', 'other'] }), on({ categories: ['meal', 'work', 'other'] })]) {
      const wrapper = await show(showing);
      const chips = wrapper.find('[data-kinds]').findAll('[data-kind]').map(c => c.text());
      expect(chips[0]).toContain('Meal');
      expect(chips[2]).toBe('Other');
      // "other" is not one anybody can drop: it is where what fits nowhere goes.
      expect(wrapper.find('[data-kinds]').findAll('[data-drop-kind]')).toHaveLength(2);

      await wrapper.findAll('[data-drop-kind]')[1].trigger('click');
      await flushPromises();
      const saved = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_configure').pop()?.[1] as { settings: { categories: string[] } };
      expect(saved.settings.categories).toEqual(['meal', 'other']);
    }
  });

  it('adds a kind as the vault keeps them: trimmed, lowercased, "other" last', async () => {
    const wrapper = await show(on({ categories: ['meal', 'other'] }));
    await wrapper.find('[data-add-kind]').setValue('  Cắm Trại ');
    await wrapper.find('[data-add-kind]').trigger('keydown.enter');
    await flushPromises();
    const saved = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_configure').pop()?.[1] as { settings: { categories: string[] } };
    expect(saved.settings.categories).toEqual(['meal', 'cắm trại', 'other']);
  });

  /// Starting again takes something away, so it says what and asks twice.
  it('counts what starting again would take before it takes any of it', async () => {
    const showing = on();
    const wrapper = await show(showing);
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'timeline_extract_status') return showing;
      if (command === 'timeline_reset_plan') return { moments: 37, proposals: 66, readings: 79, decisions: 12, month_files: 10, surrogates: 3 };
      if (command === 'timeline_reset') return { moments: 37, month_files: 10, review_files: 1, surrogates_kept: 3, failed: [] };
      return null;
    });

    // Nothing happens until it has said what it would do.
    await wrapper.find('[data-reset-ask]').trigger('click');
    await flushPromises();
    const said = wrapper.find('[data-reset-plan]').text();
    expect(said).toContain('37');
    expect(said).toContain('66');
    expect(vi.mocked(invoke).mock.calls.some(c => c[0] === 'timeline_reset')).toBe(false);

    await wrapper.find('[data-reset-confirm]').trigger('click');
    await flushPromises();
    // Confirmed against the same count it showed: a moment arriving by sync
    // in between would make the answer mean something else.
    expect(vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_reset').pop()?.[1]).toMatchObject({
      vaultPath: '/vault',
      expectMoments: 37,
    });
    expect(wrapper.find('[data-reset-done]').text()).toContain('37');
    expect(wrapper.emitted('changed')).toBeTruthy();
  });

  /// Whoever else is showing the timeline is in another component tree: the
  /// settings are a modal of the app, the timeline is a screen of Nexus. The
  /// vault was cleared and Nexus went on showing every moment it had counted
  /// before, with a badge saying 90 proposals were waiting.
  it('tells the rest of the app when the timeline has been cleared', async () => {
    const showing = on();
    const wrapper = await show(showing);
    const heard: string[] = [];
    const bus = useEventBus();
    const listen = () => heard.push('timeline:changed');
    bus.on('timeline:changed', listen);

    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'timeline_extract_status') return showing;
      if (command === 'timeline_reset_plan') return { moments: 2, proposals: 0, readings: 3, decisions: 1, month_files: 2, surrogates: 0 };
      if (command === 'timeline_reset') return { moments: 2, month_files: 2, review_files: 1, surrogates_kept: 0, failed: [] };
      return null;
    });
    await wrapper.find('[data-reset-ask]').trigger('click');
    await flushPromises();
    await wrapper.find('[data-reset-confirm]').trigger('click');
    await flushPromises();
    bus.off('timeline:changed', listen);
    expect(heard).toEqual(['timeline:changed']);
  });
});
