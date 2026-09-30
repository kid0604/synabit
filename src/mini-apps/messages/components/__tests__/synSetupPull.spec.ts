import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => {}) }));

import { i18n } from '../../../../i18n';
import SynSetupCard from '../SynSetupCard.vue';
import { RECOMMENDED_LOCAL_MODEL } from '../../models';

/**
 * The first-run card downloads the model itself (brief P5): nobody is asked
 * to open a terminal and type `ollama pull`.
 */

const mounted: VueWrapper<any>[] = [];
afterEach(() => mounted.splice(0).forEach((w) => w.unmount()));
beforeEach(() => invoke.mockReset());

const card = () => {
  const w = mount(SynSetupCard, {
    props: { local: true, providerName: 'Ollama', vaultPath: '/vault' },
    global: { plugins: [i18n] },
  });
  mounted.push(w);
  return w;
};

describe('the setup card’s model download', () => {
  it('offers the download as a button', () => {
    const button = card().find('[data-syn-pull]');
    expect(button.exists()).toBe(true);
    expect(button.element.tagName).toBe('BUTTON');
  });

  it('asks Ollama for the recommended model, then checks the connection again', async () => {
    invoke.mockResolvedValue([]);
    const w = card();
    await w.find('[data-syn-pull]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_pull_model', { modelName: RECOMMENDED_LOCAL_MODEL, vaultPath: '/vault' });
    expect(w.emitted('retry')).toBeTruthy();
    expect(w.find('[data-syn-pull]').attributes('disabled')).toBeDefined();
  });

  it('says so when the download fails, and lets it be tried again', async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'syn_pull_model') throw new Error('connection refused');
    });
    const w = card();
    await w.find('[data-syn-pull]').trigger('click');
    await flushPromises();
    expect(w.find('[role="alert"]').exists()).toBe(true);
    expect(w.emitted('retry')).toBeFalsy();
    expect(w.find('[data-syn-pull]').attributes('disabled')).toBeUndefined();
  });
});
