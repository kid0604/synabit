import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import type { Routine } from '../../routines';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import RoutineProposalCard from '../RoutineProposalCard.vue';
import { refreshRoutineProposals } from '../../composables/useSynRoutineProposals';

const morning: Routine = {
  id: 'r1',
  name: 'Lịch sáng',
  ask: 'Tóm tắt lịch hôm nay.',
  schedule: { at: '07:00', weekdays: [1, 2, 3, 4, 5] },
  enabled: false,
  to_phone: true,
  author: 'syn',
  proposed_in: 'conv-1',
};

const show = (conversationId = 'conv-1') =>
  mount(RoutineProposalCard, { props: { vaultPath: '/v', conversationId }, global: { plugins: [i18n] } });

describe('The card for a routine Syn wrote', () => {
  beforeEach(async () => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => (cmd === 'syn_waiting_routine_proposals' ? [morning] : null));
    await refreshRoutineProposals('/v');
  });

  it('says what it will do, when, where it runs and what it may not do', async () => {
    const w = show();
    await flushPromises();
    const text = w.text();
    expect(text).toContain('Turn this on? “Lịch sáng”');
    expect(text).toContain('Tóm tắt lịch hôm nay.');
    expect(text).toContain('07:00');
    expect(text).toContain('Also sent to your phone.');
    expect(text).toContain('never change or remove');
  });

  it('turns it on with one press', async () => {
    const w = show();
    await flushPromises();
    await w.find('[data-test="routine-turn-on"]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_answer_routine_proposal', { vaultPath: '/v', routineId: 'r1', keep: true });
    expect(w.find('[data-test="routine-said"]').text()).toContain('“Lịch sáng” is on');
  });

  it('says what it waits for when it runs on something new', async () => {
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'syn_waiting_routine_proposals' ? [{ ...morning, schedule: { at: '', weekdays: [] }, on_new: 'bank' }] : null);
    await refreshRoutineProposals('/v');
    const w = show();
    await flushPromises();
    expect(w.text()).toContain('whenever bank brings in something new');
  });
});
