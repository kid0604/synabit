import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import en from '../../../../i18n/locales/en.json';
import vi_ from '../../../../i18n/locales/vi.json';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));

import RehearsalCard from '../RehearsalCard.vue';

const proposal = {
  id: '0123456789abcdef',
  door_name: 'Downloads',
  root: '/Users/a/Downloads',
  made: '2026-11-04T00:00:00Z',
  unchanged: 3,
  read_outside: true,
  ops: [
    { n: 0, what: 'move', from: 'lease.pdf', to: 'Docs/lease.pdf', size: 900, class: 'data', why: null },
    { n: 1, what: 'new', from: null, to: 'run.sh', size: 9, class: 'unknown', why: 'not_a_document' },
    { n: 2, what: 'new', from: null, to: 'link', size: 0, class: 'blocked', why: 'not_a_file' },
    { n: 3, what: 'trash', from: 'old.txt', to: null, size: 3, class: 'data', why: null },
  ],
};

const show = () => mount(RehearsalCard, { props: { vaultPath: '/v', conversationId: 'conv-1' }, global: { plugins: [i18n] } });

describe('the card for a rehearsal', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_waiting_rehearsals') return [proposal];
      if (cmd === 'syn_apply_rehearsal') return { confirmed: true, outcome: { journal: proposal.id, done: [0, 3], skipped: [] } };
      return null;
    });
  });

  it('chooses the documents beforehand, and nothing that is not one', async () => {
    const w = show();
    await flushPromises();
    const box = (n: number) => w.find(`[data-test="op-${n}"] input`).element as HTMLInputElement;
    expect(box(0).checked && box(3).checked).toBe(true);
    expect(box(1).checked).toBe(false);
    expect(box(2).checked).toBe(false);
    expect(box(2).disabled).toBe(true);
    expect(w.find('[data-test="rehearsal-unknown"]').text()).toContain('run.sh');
    expect(w.text()).toContain(en.syn.rehearsal_read);
  });

  it('applies what is chosen, in the screen language, and says what came of it', async () => {
    const w = show();
    await flushPromises();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_waiting_rehearsals') return [];
      if (cmd === 'syn_apply_rehearsal') return { confirmed: true, outcome: { journal: proposal.id, done: [0, 3], skipped: [] } };
      return null;
    });
    await w.find('[data-test="rehearsal-apply"]').trigger('click');
    await flushPromises();
    const call = invoke.mock.calls.find(([cmd]) => cmd === 'syn_apply_rehearsal');
    expect(call?.[1]).toMatchObject({ vaultPath: '/v', id: proposal.id, locale: 'en' });
    expect([...call?.[1].approved].sort()).toEqual([0, 3]);
    expect(w.find('[data-test="rehearsal-result"]').text()).toContain('2 applied');
  });

  it('says nothing changed when the system dialog was answered no', async () => {
    const w = show();
    await flushPromises();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_waiting_rehearsals') return [];
      if (cmd === 'syn_apply_rehearsal') return { confirmed: false, outcome: null };
      return null;
    });
    await w.find('[data-test="rehearsal-apply"]').trigger('click');
    await flushPromises();
    expect(w.text()).toContain(en.syn.rehearsal_cancelled);
  });

  it('has words for every op and every reason, in both languages', () => {
    for (const lang of [en, vi_]) {
      for (const what of ['rename', 'move', 'trash', 'new', 'change', 'new_folder', 'remove_folder']) {
        expect(lang.syn, what).toHaveProperty(`rehearsal_what_${what}`);
      }
      for (const why of ['not_a_document', 'executable', 'content_does_not_match', 'name_taken', 'second_name', 'not_a_file', 'changed_since', 'too_large']) {
        expect(lang.syn, why).toHaveProperty(`rehearsal_why_${why}`);
      }
    }
  });
});
