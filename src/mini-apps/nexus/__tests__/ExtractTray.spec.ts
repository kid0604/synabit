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
    // `title: null` — the sentence was left as the model wrote it.
    expect(invoke).toHaveBeenCalledWith('timeline_extract_review', {
      vaultPath: '/vault',
      itemId: 'x1',
      accept: true,
      nodeId: 'Notes/2024-06-02.md',
      title: null,
    });
    expect(wrapper.findAll('[data-proposal]')).toHaveLength(0);
    expect(wrapper.emitted('changed')).toHaveLength(1);
  });

  // ─── A proposal that is nearly right ────────────────────────

  const withProposal = () =>
    mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false },
      proposals: [proposal],
    }));

  const lastReview = () =>
    vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_review').pop()?.[1];

  /// Keep-or-discard makes a nearly right proposal either kept wrong or
  /// thrown away. This is the third thing.
  it('can be reworded before it is kept', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-reword]').trigger('click');

    const box = wrapper.find('[data-proposal-title]');
    expect((box.element as HTMLTextAreaElement).value).toBe(proposal.title);
    await box.setValue('Took Mum to her eye appointment');
    await wrapper.find('[data-accept]').trigger('click');
    await flushPromises();

    expect(lastReview()).toMatchObject({
      itemId: 'x1',
      accept: true,
      title: 'Took Mum to her eye appointment',
    });
  });

  /// Opening the box and leaving it as it was is not a decision. Sending the
  /// same sentence back would record one nobody made.
  it('sends no rewording when the sentence was not changed', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-reword]').trigger('click');
    await wrapper.find('[data-proposal-title]').setValue(`  ${proposal.title}  `);
    await wrapper.find('[data-accept]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({ title: null });
  });

  /// The quote is the line in the note this was read from — what makes the
  /// whole of extraction answerable. It is shown, and it is not a box.
  it('shows the evidence and offers no way to rewrite it', async () => {
    const wrapper = await withProposal();
    expect(wrapper.text()).toContain(proposal.quote);
    await wrapper.find('[data-reword]').trigger('click');
    expect(wrapper.findAll('textarea')).toHaveLength(1);
  });

  it('discards without asking about the sentence', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-reword]').trigger('click');
    await wrapper.find('[data-proposal-title]').setValue('does not matter');
    await wrapper.find('[data-decline]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({ accept: false, title: null });
  });
});