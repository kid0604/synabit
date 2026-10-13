import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockResolvedValue(() => {}) }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));

import SynSetupCard from '../SynSetupCard.vue';

const show = () =>
  mount(SynSetupCard, { props: { local: true, providerName: 'Ollama', vaultPath: '/v' }, global: { plugins: [i18n] } });

describe('The first-run card', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    invoke.mockReset();
  });

  it('connects with nothing but a pasted key, and says to what', async () => {
    invoke.mockImplementation(async (cmd: string) =>
      cmd === 'syn_connect_with_key' ? { provider: 'anthropic', model: 'claude-sonnet-4-5' } : null);
    const w = show();
    await w.find('[data-syn-key-input]').setValue('sk-ant-api03-abcdefghijklmnopqrstuvwxyz');
    await w.find('form').trigger('submit');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_connect_with_key', { vaultPath: '/v', key: 'sk-ant-api03-abcdefghijklmnopqrstuvwxyz' });
    expect(w.find('[data-syn-key-done]').text()).toContain('Connected to Anthropic, using claude-sonnet-4-5');
    expect(w.emitted('connected')?.[0]).toEqual(['anthropic', 'claude-sonnet-4-5']);
    // Where notes go is said before the button, not after.
    expect(w.find('[data-syn-key]').text()).toContain('goes to that company');
  });

  it('says plainly what is wrong with a key it does not know', async () => {
    invoke.mockImplementation(async () => { throw 'unrecognised_key'; });
    const w = show();
    await w.find('[data-syn-key-input]').setValue('ghp_not_a_model_key_at_all_123');
    await w.find('form').trigger('submit');
    await flushPromises();
    expect(w.find('[data-syn-key-error]').text()).toContain("doesn't look like a key");
    expect(w.emitted('connected')).toBeUndefined();
  });

  it('keeps running it here one line away', () => {
    expect(show().text()).toContain('Or run it on this computer, with Ollama');
  });
});
