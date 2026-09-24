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
  category: 'health',
  amount: null,
  about: [],
  time: null,
  place: null,
  date_basis: 'relative',
  about_moment: null,
  verdict: null,
};

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

  it('never reads anything just by being opened', async () => {
    await mountTray(status({ config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] } }));
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).not.toContain('timeline_extract_run');
  });

  it('keeps a proposal only when asked, and says where it came from', async () => {
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [proposal],
    }));
    const row = wrapper.find('[data-proposal]');
    expect(row.text()).toContain('Yesterday I took Mum to the eye clinic');
    expect(row.text()).toContain('2024-06-01 · Mum');

    await row.find('[data-accept]').trigger('click');
    await flushPromises();
    // `edits: null` — nothing was put right, so nothing is the person's.
    expect(invoke).toHaveBeenCalledWith('timeline_extract_review', {
      vaultPath: '/vault',
      itemId: 'x1',
      accept: true,
      nodeId: 'Notes/2024-06-02.md',
      edits: null,
      assigned: [],
    });
    expect(wrapper.findAll('[data-proposal]')).toHaveLength(0);
    expect(wrapper.emitted('changed')).toHaveLength(1);
  });

  // ─── A proposal that is nearly right ────────────────────────

  const withProposal = () =>
    mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [proposal],
    }));

  const lastReview = () =>
    vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_review').pop()?.[1];

  /// Keep-or-discard makes a nearly right proposal either kept wrong or
  /// thrown away. This is the third thing.
  /// Every field, not only the sentence: a proposal is wrong in whichever
  /// field it is wrong in, and the day and the people are the ones that make
  /// it worth keeping at all.
  it('can be put right in full before it is kept', async () => {
    const wrapper = await withProposal();
    await wrapper.find('[data-edit]').trigger('click');

    const box = wrapper.find('[data-proposal-title]');
    expect((box.element as HTMLTextAreaElement).value).toBe(proposal.title);
    await box.setValue('Took Mum to her eye appointment');
    await wrapper.find('[data-field-from]').setValue('2024-06-02');
    await wrapper.find('[data-field-time]').setValue('09:30');
    await wrapper.find('[data-field-where]').setValue('Mắt Trung ương');
    await wrapper.find('[data-field-category]').setValue('health');
    await wrapper.find('[data-field-amount]').setValue('300000');
    await wrapper.find('[data-add-person]').setValue('bác sĩ Long');
    await wrapper.find('[data-add-person]').trigger('keydown.enter');
    await wrapper.find('[data-accept]').trigger('click');
    await flushPromises();

    expect(lastReview()).toMatchObject({
      itemId: 'x1',
      accept: true,
      edits: {
        title: 'Took Mum to her eye appointment',
        happened_from: '2024-06-02',
        happened_to: '2024-06-01',
        time: '09:30',
        people: ['p-me', 'bác sĩ Long'],
        place: 'Mắt Trung ương',
        category: 'health',
        amount: 300000,
      },
    });
  });

  /// Saying who a name is teaches the next reading: it becomes that person's
  /// alias, and nobody is asked who "Cam" is twice.
  it('can say who a name nobody matched belongs to', async () => {
    const unmatched: Proposal = { ...proposal, people: [], names: ['Cam'] };
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [unmatched],
      people: [{ id: 'People/cam.md', title: 'Cam Nguyễn' }],
    }));
    await wrapper.find('[data-edit]').trigger('click');
    await wrapper.find('[data-assign]').setValue('People/cam.md');
    await wrapper.find('[data-accept]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({
      assigned: [{ name: 'Cam', node: 'People/cam.md' }],
      edits: { people: ['People/cam.md'] },
    });
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
    expect(lastReview()).toMatchObject({ accept: false, edits: null });
  });

  /// A day that reads right the way it stands is kept as a day.
  it('keeps a whole day at once', async () => {
    const second: Proposal = { ...proposal, id: 'x2', title: 'Lunch with Nga' };
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [proposal, second],
    }));
    expect(wrapper.findAll('[data-day]')).toHaveLength(1);
    await wrapper.find('[data-keep-day]').trigger('click');
    await flushPromises();
    const kept = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_review');
    expect(kept.map(c => (c[1] as { itemId: string }).itemId)).toEqual(['x1', 'x2']);
  });

  /// A pass of twenty is not a pass done by pointing at three buttons a card.
  it('moves and decides from the keyboard', async () => {
    const second: Proposal = { ...proposal, id: 'x2', title: 'Lunch with Nga' };
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [proposal, second],
    }));
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j' }));
    await flushPromises();
    expect(wrapper.find('[data-selected]').attributes('data-proposal')).toBe('x1');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'j' }));
    await flushPromises();
    expect(wrapper.find('[data-selected]').attributes('data-proposal')).toBe('x2');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
    await flushPromises();
    expect(lastReview()).toMatchObject({ itemId: 'x2', accept: true });
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'x' }));
    await flushPromises();
    expect(lastReview()).toMatchObject({ accept: false });
  });

  /// The kinds a moment can be are the vault's, not the app's: a life is not
  /// a list somebody else wrote.
  it('offers the kinds the vault keeps, and lets one be added where it is missing', async () => {
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [proposal],
      categories: ['ăn uống', 'sự cố', 'other'],
    }));
    await wrapper.find('[data-edit]').trigger('click');
    const options = wrapper.find('[data-field-category]').findAll('option').map(o => o.text());
    expect(options).toEqual(['ăn uống', 'sự cố', 'Other', 'add a kind…']);

    // Adding one saves it to the vault, and "other" stays last.
    await wrapper.find('[data-field-category]').setValue('__new');
    await wrapper.find('[data-new-kind]').setValue('  Cắm Trại ');
    await wrapper.find('[data-new-kind]').trigger('keydown.enter');
    await flushPromises();
    const saved = vi.mocked(invoke).mock.calls.filter(c => c[0] === 'timeline_extract_configure').pop()?.[1] as { settings: { categories: string[] } };
    expect(saved.settings.categories).toEqual(['ăn uống', 'sự cố', 'cắm trại', 'other']);
  });

  /// An empty queue is not a dead end: whatever you do next about the
  /// timeline — turn reading on, run it, start over — is in the settings, and
  /// this says so rather than leaving you to guess.
  it('points at the settings when there is nothing waiting', async () => {
    for (const showing of [status(), status({ config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] } })]) {
      const wrapper = await mountTray(showing);
      expect(wrapper.find('[data-nothing-waiting]').exists()).toBe(true);
      await wrapper.find('[data-open-settings]').trigger('click');
      expect(wrapper.emitted('settings')).toBeTruthy();
    }
  });

  /// A change to a moment already kept: what it says now against what the
  /// note says now, and the fields the person wrote left out of it (§15).
  it('shows a change to a moment already kept, and what it would change', async () => {
    const change: Proposal = {
      ...proposal,
      id: 'u1',
      title: 'Took Mum and Dad to the eye clinic',
      people: [{ id: 'p-me', title: 'Mum' }, { id: 'p-dad', title: 'Dad' }],
      about_moment: 'Moments/6f3c.md',
      verdict: 'changed',
    };
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [change],
      moments: {
        'Moments/6f3c.md': {
          path: 'Moments/6f3c.md',
          title: 'A sentence of my own',
          happened: '2024-06-01',
          people: ['Mum'],
          place: null,
          category: 'health',
          time: null,
          amount: null,
          hand: ['title'],
        },
      },
    }));
    const card = wrapper.find('[data-proposal]');
    expect(card.find('[data-change]').text()).toBe('The source changed');
    expect(card.text()).toContain('A sentence of my own');
    const rows = card.find('[data-diff]').text();
    expect(rows).toContain('Mum, Dad');
    expect(rows).not.toContain('Took Mum and Dad to the eye clinic');
    expect(card.find('[data-hand]').text()).toContain('Title');

    await card.find('[data-accept]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({ itemId: 'u1', accept: true });
  });

  /// The words it stood on are gone: keeping it is a decision, letting it go
  /// is the other one, and neither is made for the person.
  it('offers to keep or let go a moment whose words are gone', async () => {
    const gone: Proposal = { ...proposal, id: 'u2', about_moment: 'Moments/6f3c.md', verdict: 'retracted' };
    const wrapper = await mountTray(status({
      config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
      proposals: [gone],
      moments: {
        'Moments/6f3c.md': { path: 'Moments/6f3c.md', title: 'Took Mum to the eye clinic', happened: '2024-06-01', people: [], place: null, category: null, time: null, amount: null, hand: [] },
      },
    }));
    const card = wrapper.find('[data-proposal]');
    expect(card.find('[data-change]').text()).toBe('The words are gone');
    await card.find('[data-decline]').trigger('click');
    await flushPromises();
    expect(lastReview()).toMatchObject({ itemId: 'u2', accept: false });
  });
});
describe('Checking a proposal against the note it came from', () => {
  const NOTE = 'Sáng nay đi chợ.\nYesterday I took Mum to the eye clinic and she was fine.\nRồi về nấu cơm.';

  const withNote = async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'timeline_extract_status') {
        return status({
          config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
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
  /// What the reader read besides the day and the people: the kind of
  /// thing, where, how much, and whether the day was only a guess.
  it('says what kind of moment it is, where, and what it cost', async () => {
    const lunch: Proposal = {
      ...proposal, id: 'x2', title: 'Lunch with Nga', category: 'meal', place: 'Hàng Mành',
      amount: { value: 50000, unit: 'VND' }, time: '12:15', date_basis: 'inferred',
    };
    const wrapper = await mountTray(status({ config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] }, proposals: [lunch] }));
    const card = wrapper.find('[data-proposal]');
    expect(card.text()).toContain('2024-06-01 12:15');
    const details = card.find('[data-proposal-details]').text();
    expect(details).toContain('Meal');
    expect(details).toContain('Hàng Mành');
    expect(details).toMatch(/50,000|50\.000/);
    expect(card.find('[data-day-guessed]').exists()).toBe(true);
    expect(card.text()).not.toMatch(/\d+%/);
  });

  it('shows the words, not the link syntax', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === 'timeline_extract_status'
        ? status({
            config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
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
