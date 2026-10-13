import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import type { ChangesEvent } from '../../composables/useSynChanges';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import HeldChangesCard from '../HeldChangesCard.vue';

const waiting: ChangesEvent = {
  run_id: 'run-1234abcd',
  conversation_id: 'conv-1',
  changes: {
    because: ['bao-cao-q3.pdf'],
    changes: [
      {
        hash: 'aaaaaaaa1',
        tool: 'update_node',
        kind: 'edit',
        titles: ['Dự án Q3'],
        fields: [{ name: 'status', before: 'open', after: 'done' }],
        before: 'cũ',
        after: 'mới',
        state: 'held',
      },
      { hash: 'bbbbbbbb2', tool: 'trash_node', kind: 'trash', titles: ['Giữ lại'], fields: [], state: 'held' },
    ],
  },
};

const show = (conversationId = 'conv-1') =>
  mount(HeldChangesCard, { props: { vaultPath: '/v', conversationId }, global: { plugins: [i18n] } });

describe('The held changes card', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_waiting_changes') return [waiting];
      if (cmd === 'syn_answer_changes') return [{ hash: 'aaaaaaaa1', state: 'applied' }, { hash: 'bbbbbbbb2', state: 'declined' }];
      return null;
    });
  });

  it('says what each change does, to what, and what was read that made it wait', async () => {
    const w = show();
    await flushPromises();
    const text = w.text();
    expect(text).toContain('Syn wants to change 2 things');
    expect(text).toContain('bao-cao-q3.pdf');
    expect(text).toContain('Change: Dự án Q3');
    expect(text).toContain('Move to the trash: Giữ lại');
    expect(text).toContain('open');
    expect(text).toContain('done');
    for (const word of ['update_node', 'trash_node', 'staged', 'taint', 'tool']) {
      expect(text.toLowerCase()).not.toContain(word);
    }
  });

  it('makes only what is ticked, and says what came of it', async () => {
    const w = show();
    await flushPromises();
    await w.find('[data-test="change-bbbbbbbb"] input').setValue(false);
    expect(w.find('[data-test="changes-apply"]').text()).toBe('Make 1 change');
    await w.find('[data-test="changes-apply"]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_answer_changes', { vaultPath: '/v', runId: 'run-1234abcd', approved: ['aaaaaaaa1'] });
    expect(w.find('[data-test="held-run-1234"]').exists()).toBe(false);
    expect(w.find('[data-test="changes-result"]').text()).toContain('1 made');
  });

  it('belongs to its own conversation only', async () => {
    const w = show('conv-other');
    await flushPromises();
    expect(w.find('[data-test="held-run-1234"]').exists()).toBe(false);
  });
});
