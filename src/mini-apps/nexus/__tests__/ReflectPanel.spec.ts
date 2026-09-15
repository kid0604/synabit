import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import ReflectPanel, { type Decision, type Overview, type Pattern } from '../components/ReflectPanel.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const decision = (id: string, overrides: Partial<Decision> = {}): Decision => ({
  id: `Decisions/${id}.md`,
  title: `Decision ${id}`,
  decided_on: '2025-01-10',
  expected: 'a calmer year',
  review_on: '2025-09-01',
  reasoning: 'Burnt out.',
  tags: ['job'],
  reviews: [],
  due: false,
  ...overrides,
});

const mountPanel = async (overview: Overview, answers: Record<string, unknown> = {}) => {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === 'reflect_overview') return overview;
    return answers[command] ?? null;
  });
  const wrapper = mount(ReflectPanel, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
  await flushPromises();
  await wrapper.find('button').trigger('click');
  await flushPromises();
  return wrapper;
};

describe('ReflectPanel', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('switched off, offers the switch and nothing else', async () => {
    const wrapper = await mountPanel({ enabled: false, decisions: [], groups: [] });
    expect(wrapper.find('[data-reflect-enable]').exists()).toBe(true);
    expect(wrapper.find('[data-write]').exists()).toBe(false);
    expect(wrapper.find('[data-group]').exists()).toBe(false);

    await wrapper.find('[data-reflect-enable]').trigger('click');
    expect(invoke).toHaveBeenCalledWith('reflect_configure', { vaultPath: '/vault', enabled: true });
  });

  it('asks what actually happened when a decision is due', async () => {
    const wrapper = await mountPanel({ enabled: true, decisions: [decision('leave', { due: true })], groups: [] });
    expect(wrapper.find('[data-due-count]').text()).toBe('1');

    const due = wrapper.find('[data-due]');
    await due.find('[data-happened]').setValue('Calmer, and poorer.');
    await due.find('[data-outcome="partly"]').trigger('click');
    await due.find('[data-save-review]').trigger('click');
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('reflect_add_review', {
      vaultPath: '/vault',
      id: 'Decisions/leave.md',
      happened: 'Calmer, and poorer.',
      outcome: 'partly',
      nextReviewOn: null,
    });
  });

  it('offers no pattern before three decisions have been looked back on', async () => {
    const wrapper = await mountPanel({
      enabled: true,
      decisions: [decision('a'), decision('b')],
      groups: [{ tag: 'job', cases: ['Decisions/a.md', 'Decisions/b.md'], tally: { as_expected: 0, partly: 1, otherwise: 1, unrated: 0 }, enough: false }],
    });
    expect(wrapper.find('[data-group]').exists()).toBe(true);
    expect(wrapper.find('[data-look-for-pattern]').exists()).toBe(false);
  });

  it('shows a pattern as an observation, each sentence with the decisions it rests on', async () => {
    const pattern: Pattern = {
      tag: 'job',
      sentences: [{ text: 'All three were decided within a week.', sources: [1, 2, 3] }],
      sources: ['a', 'b', 'c'].map((id, i) => ({ n: i + 1, node_id: `Decisions/${id}.md`, node_type: 'decision', title: `Decision ${id}`, date: '2020-01-01', what: '' })),
      dropped: 0,
      advice_dropped: 1,
      cited: 3,
      withheld: null,
    };
    const wrapper = await mountPanel(
      {
        enabled: true,
        decisions: [decision('a'), decision('b'), decision('c')],
        groups: [{ tag: 'job', cases: ['Decisions/a.md', 'Decisions/b.md', 'Decisions/c.md'], tally: { as_expected: 1, partly: 0, otherwise: 2, unrated: 0 }, enough: true }],
      },
      { reflect_pattern: pattern },
    );
    await wrapper.find('[data-look-for-pattern]').trigger('click');
    await flushPromises();

    const shown = wrapper.find('[data-pattern]');
    expect(shown.text()).toContain('Observation, not advice');
    expect(shown.findAll('[data-citation]')).toHaveLength(3);
    expect(shown.text()).toContain('1 sentences were left out');

    await shown.findAll('[data-citation]')[1].trigger('click');
    expect(wrapper.find('[data-expanded="true"]').text()).toContain('Decision b');
  });

  it('opens on the decision a reminder was about', async () => {
    vi.mocked(invoke).mockImplementation(async () => ({ enabled: true, decisions: [decision('a'), decision('b')], groups: [] }));
    const wrapper = mount(ReflectPanel, { props: { vaultPath: '/vault', focus: 'Decisions/b.md' }, global: { plugins: [i18n] } });
    await flushPromises();
    expect(wrapper.find('[data-reflect-panel]').exists()).toBe(true);
    expect(wrapper.find('[data-expanded="true"]').text()).toContain('Decision b');
  });
});
