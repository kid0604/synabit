import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
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
  // No button to press any more: the tray is the content of a screen of its
  // own, and `NexusApp` decides when that screen is shown.
  const wrapper = mount(ExtractTray, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
  await flushPromises();
  return wrapper;
};

describe('ExtractTray', () => {
  beforeEach(() => {
  // The tray reads the app lock before it shows a note's words.
  setActivePinia(createPinia());
  vi.mocked(invoke).mockReset();
});

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
  it('can be edited before it is kept', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-edit]').trigger('click');

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
  it('sends no edit when the sentence was not changed', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-edit]').trigger('click');
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
    await wrapper.find('[data-edit]').trigger('click');
    expect(wrapper.findAll('textarea')).toHaveLength(1);
  });

  it('discards without asking about the sentence', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-edit]').trigger('click');
    await wrapper.find('[data-proposal-title]').setValue('does not matter');
    await wrapper.find('[data-decline]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({ accept: false, title: null });
  });
});
describe('Checking a proposal against the note it came from', () => {
  const NOTE = 'Sáng nay đi chợ.\nYesterday I took Mum to the eye clinic and she was fine.\nRồi về nấu cơm.';

  const withNote = async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'timeline_extract_status') {
        return status({
          config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false },
          proposals: [proposal],
        });
      }
      if (command === 'get_nexus_item') return { content: NOTE };
      return null;
    });
    const wrapper = mount(ExtractTray, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
    await flushPromises();
    return wrapper;
  };

  /// Some of these notes are months old, and the card used to say only
  /// «From …, written …». Keeping one then meant trusting it.
  it('shows the note behind the proposal without leaving the queue', async () => {
    const wrapper = await withNote();
    expect(wrapper.find('[data-source]').exists()).toBe(false);

    await wrapper.find('[data-show-source]').trigger('click');
    await flushPromises();

    const source = wrapper.find('[data-source]');
    expect(source.text()).toContain('Sáng nay đi chợ');
    expect(source.text()).toContain('Rồi về nấu cơm');
    // The queue is still there: nothing navigated away.
    expect(wrapper.findAll('[data-proposal]')).toHaveLength(1);
  });

  /// The line the model read is marked, so the eye goes to it rather than
  /// hunting through a month of a daily note.
  it('marks the line it was read from', async () => {
    const wrapper = await withNote();
    await wrapper.find('[data-show-source]').trigger('click');
    await flushPromises();
    expect(wrapper.find('[data-hit]').text()).toBe('Yesterday I took Mum to the eye clinic');
  });

  it('folds away when pressed again', async () => {
    const wrapper = await withNote();
    await wrapper.find('[data-show-source]').trigger('click');
    await flushPromises();
    await wrapper.find('[data-show-source]').trigger('click');
    expect(wrapper.find('[data-source]').exists()).toBe(false);
  });

  /// Reading is enough to decide; changing the note is worth leaving for.
  it('offers the note itself, at the line', async () => {
    const wrapper = await withNote();
    await wrapper.find('[data-show-source]').trigger('click');
    await flushPromises();
    await wrapper.find('[data-open-source]').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual([
      'Notes/2024-06-02.md',
      'note',
      'Yesterday I took Mum to the eye clinic',
    ]);
  });
});

describe('A quote that carries markup', () => {
  /// Straight from a real vault: the model quoted a line whose person link
  /// was still written out, so the card read
  /// «Trao đổi với [Nguyễn Lê Vũ Phương Hoàng:](synabit://person/People/77a…md)»
  /// — the name was there, buried in forty characters of uuid.
  it('shows the words, not the link syntax', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === 'timeline_extract_status'
        ? status({
            config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false },
            proposals: [{
              ...proposal,
              quote: 'Trao đổi với [Nguyễn Lê Vũ Phương Hoàng:](synabit://person/People/77a70830-72a1-4548-a6ef-4e8fafff8d44.md)',
            }],
          })
        : null);
    const wrapper = mount(ExtractTray, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
    await flushPromises();

    const row = wrapper.find('[data-proposal]');
    expect(row.text()).toContain('Trao đổi với Nguyễn Lê Vũ Phương Hoàng:');
    expect(row.text()).not.toContain('synabit://');
    expect(row.text()).not.toContain('77a70830');
  });
});
