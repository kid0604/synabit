import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import type { SkillProposal } from '../../composables/useSynSkillProposals';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import SkillProposalCard from '../SkillProposalCard.vue';
import { refreshSkillProposals } from '../../composables/useSynSkillProposals';

const weekly: SkillProposal = {
  skill_id: 'SynSkills/Tổng kết tuần.md',
  conversation_id: 'conv-1',
  name: 'Tổng kết tuần',
  description: 'Tìm task xong trong tuần và viết một note tổng kết.',
  when_to_use: 'khi bạn nói “tổng kết tuần”',
  tier: 'recipe',
  steps: '1. Tìm task xong.\n2. Viết note.',
  calls: [{ tool: 'query_nodes', does: 'read' }, { tool: 'create_node', does: 'write' }],
  read_before: [],
  version: 1,
};

const show = (conversationId = 'conv-1') =>
  mount(SkillProposalCard, { props: { vaultPath: '/v', conversationId }, global: { plugins: [i18n] } });

describe('The card for a skill Syn wrote', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_waiting_skill_proposals' ? [weekly] : null));
  });

  it('says what it does, when, and what each step touches, in words', async () => {
    const w = show();
    await flushPromises();
    const text = w.text();
    expect(text).toContain('Tổng kết tuần');
    expect(text).toContain('Syn would use it when: khi bạn nói “tổng kết tuần”');
    expect(text).toContain('changes things');
    for (const word of ['query_nodes', 'create_node', 'recipe', 'prose', 'tool']) {
      expect(text.toLowerCase()).not.toContain(word);
    }
  });

  it('keeps it with one press, and then says so', async () => {
    const w = show();
    await flushPromises();
    await w.find('[data-test="skill-keep"]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_answer_skill_proposal', { vaultPath: '/v', skillId: weekly.skill_id, keep: true });
    expect(w.find('[data-test="skill-said"]').text()).toContain('Kept “Tổng kết tuần”');
  });

  it('names what was read before it was written', async () => {
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'syn_waiting_skill_proposals' ? [{ ...weekly, read_before: ['tips.example'] }] : null);
    // The store outlives a card; read again, as the app does on opening.
    await refreshSkillProposals();
    const w = show();
    await flushPromises();
    expect(w.text()).toContain('Syn read tips.example before writing this');
  });

  it('belongs to its own conversation only', async () => {
    const w = show('conv-other');
    await flushPromises();
    expect(w.find('[data-test="skill-proposals"]').exists()).toBe(false);
  });
});
