import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import ExtractTray, { type ExtractStatus, type Proposal } from '../components/ExtractTray.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const proposal: Proposal = {
  id: 'x1',
  node_id: 'Notes/2024-06-02.md',
  node_title: '2024-06-02',
  node_type: 'note',
  recorded: '2024-06-02',
  happened_from: '2024-06-01',
  happened_to: '2024-06-01',
  precision: 'day',
  title: 'Took Mum to the eye clinic',
  people: [{ id: 'p-me', title: 'Mum' }],
  names: [],
  quote: 'Yesterday I took Mum to the eye clinic',
  confidence: 0.9,
  model: 'gemma',
  stale: false,
};

const status = (overrides: Partial<ExtractStatus> = {}): ExtractStatus => ({
  config: { enabled: false, allow_cloud: false, folders: [], tags: [], conversations: false },
  syn_enabled: true,
  provider: 'ollama',
  local: true,
  model: 'gemma',
  desktop: true,
  running: false,
  pending: 12,
  stale: 0,
  old_version: 0,
  done: 0,
  estimate_ms: 180_000,
  estimate_all_ms: 180_000,
  estimate_measured: false,
  unreadable: [],
  proposals: [],
  ...overrides,
});

const mountTray = async (initial: ExtractStatus) => {
  vi.mocked(invoke).mockImplementation(async (command: string) => (command === 'timeline_extract_status' ? initial : null));
  const wrapper = mount(ExtractTray, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
  await flushPromises();
  await wrapper.find('button').trigger('click');
  await flushPromises();
  return wrapper;
};

describe('ExtractTray', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('does not turn on a cloud provider until sending notes there is allowed', async () => {
    const wrapper = await mountTray(status({ local: false, provider: 'gemini' }));
    expect(wrapper.find('[data-cloud-warning]').text()).toContain('gemini');
    expect(wrapper.find('[data-enable]').attributes('disabled')).toBeDefined();

    await wrapper.find('[data-allow-cloud]').setValue(true);
    await wrapper.find('[data-enable]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('timeline_extract_configure', expect.objectContaining({
      settings: expect.objectContaining({ enabled: true, allow_cloud: true }),
    }));
  });

  it('never reads anything just by being opened', async () => {
    await mountTray(status({ config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false } }));
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).not.toContain('timeline_extract_run');
  });

  it('keeps a proposal only when asked, and says where it came from', async () => {
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false },
      proposals: [proposal],
    }));
    expect(wrapper.find('[data-proposal-count]').text()).toBe('1');
    const row = wrapper.find('[data-proposal]');
    expect(row.text()).toContain('Yesterday I took Mum to the eye clinic');
    expect(row.text()).toContain('2024-06-01 · Mum');

    await row.find('[data-accept]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('timeline_extract_review', { vaultPath: '/vault', itemId: 'x1', accept: true, nodeId: 'Notes/2024-06-02.md' });
    expect(wrapper.findAll('[data-proposal]')).toHaveLength(0);
    expect(wrapper.emitted('changed')).toHaveLength(1);
  });
});
