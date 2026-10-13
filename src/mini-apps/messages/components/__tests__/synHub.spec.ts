import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('vue-router', () => ({ useRoute: () => ({ query: {} }) }));

import SynHub from '../SynHub.vue';

describe('The Syn page', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
    invoke.mockResolvedValue([]);
  });

  it('groups what is the person\'s about Syn, in their words', () => {
    const text = mount(SynHub, { props: { vaultPath: '/v' }, global: { plugins: [i18n], stubs: { RunInspector: true, InstructionsPanel: true, RoutinesPanel: true, SynConnections: true } } }).text();
    for (const group of ['What Syn knows', 'What Syn can do', 'What Syn does on its own', 'What Syn may do']) {
      expect(text).toContain(group);
    }
    for (const word of ['inspector', 'tool', 'token', 'MCP']) expect(text.toLowerCase()).not.toContain(word.toLowerCase());
  });

  it('opens on the tab it was sent to, and the model is in Settings', async () => {
    const w = mount(SynHub, {
      props: { vaultPath: '/v', tab: 'routines' },
      global: { plugins: [i18n], stubs: { RunInspector: true, InstructionsPanel: true, RoutinesPanel: true, SynConnections: true } },
    });
    expect(w.find('[data-hub-tab="routines"]').attributes('aria-pressed')).toBe('true');
    expect(w.findComponent({ name: 'RoutinesPanel' }).exists()).toBe(true);
    await w.find('[data-hub-tab="brain"]').trigger('click');
    await flushPromises();
    expect(w.emitted('open-settings')).toBeTruthy();
  });
});
