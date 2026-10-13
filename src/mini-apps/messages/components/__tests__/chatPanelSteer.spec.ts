import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue([]) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('vue-router', () => ({ useRoute: () => ({ query: {} }), useRouter: () => ({ push: vi.fn() }) }));

import ChatPanel from '../ChatPanel.vue';

// jsdom has no scrolling; the panel scrolls to the newest message.
Element.prototype.scrollTo = () => {};

const panel = (props: Record<string, unknown>) =>
  mount(ChatPanel, {
    props: { messages: [], streamingContent: '', isStreaming: false, ...props },
    global: { plugins: [i18n], stubs: { MessageBubble: true, RunProgress: true, StreamingIndicator: true, HeldChangesCard: true, SkillProposalCard: true, RoutineProposalCard: true } },
  });

describe('Talking to Syn while it works', () => {
  it('hands words typed mid-run to that run, and stop is still there when nothing is typed', async () => {
    i18n.global.locale.value = 'en';
    const w = panel({ isStreaming: true });
    const box = w.find('textarea');
    expect(box.attributes('disabled')).toBeUndefined();
    expect(w.find(`[title="${i18n.global.t('syn.stop')}"]`).exists()).toBe(true);
    await box.setValue('no, the other note');
    await box.trigger('keydown', { key: 'Enter' });
    expect(w.emitted('steer')?.[0]).toEqual(['no, the other note']);
    expect(w.emitted('send')).toBeUndefined();
  });

  it('offers to carry on work that stopped short', async () => {
    i18n.global.locale.value = 'en';
    const w = panel({ messages: [{ id: 'm1', role: 'user', content: 'x', timestamp: '' }], continuable: { why: 'ceiling' } });
    expect(w.find('[data-carry-on]').text()).toContain('reached its limit');
    await w.find('[data-carry-on] button').trigger('click');
    expect(w.emitted('carry-on')).toBeTruthy();
  });
});
