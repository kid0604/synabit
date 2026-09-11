import { describe, it, expect, vi, beforeEach } from 'vitest';
import { nextTick } from 'vue';

const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown> = {}) => {
    calls.push({ cmd, args });
    if (cmd === 'syn_get_settings') return { provider: 'open_ai_compat' };
    if (cmd === 'syn_has_api_key') return args.provider === 'gemini';
    return undefined;
  }),
}));

const { useSynSettings, takesKey } = await import('../useSynSettings');
import settingsPanel from '../../components/SynSettings.vue?raw';
import en from '../../../../i18n/locales/en.json';
import vi_ from '../../../../i18n/locales/vi.json';

const flush = async () => {
  await nextTick();
  await new Promise(r => setTimeout(r, 0));
};

beforeEach(() => { calls.length = 0; });

/**
 * A key per provider, and the field always meaning the one that is selected.
 *
 * The key handling was hard-wired to the OpenAI-compatible provider, which was
 * right while it was the only provider with a key. With Gemini beside it, the
 * same field has to mean two different slots — and the moment where it means
 * the wrong one is the moment the selector changes.
 */
describe('the API key for the selected provider', () => {
  it('knows which providers take one', () => {
    expect(takesKey('ollama')).toBe(false);
    expect(takesKey('open_ai_compat')).toBe(true);
    expect(takesKey('gemini')).toBe(true);
  });

  it('asks about the key of the provider that is selected', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    expect(s.hasApiKey.value).toBe(false);

    s.settings.value.provider = 'gemini';
    await flush();
    expect(s.hasApiKey.value, 'the Gemini slot has one').toBe(true);
    // Not `.at(-1)`: this project's TypeScript target predates it.
    const asked = calls.filter(c => c.cmd === 'syn_has_api_key');
    expect(asked[asked.length - 1]?.args.provider).toBe('gemini');
  });

  /**
   * Type an OpenAI key, change the selector to Gemini, press Save. Without this
   * the OpenAI key is filed as Gemini's, and fails with a message about a key
   * the person is sure they typed correctly. They did — into the other slot.
   */
  it('throws away a half-typed key when the provider changes', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();

    s.apiKeyDraft.value = 'sk-openai-xxxx';
    s.settings.value.provider = 'gemini';
    await flush();
    expect(s.apiKeyDraft.value).toBe('');

    await s.saveSettings();
    expect(calls.some(c => c.cmd === 'syn_set_api_key'), 'nothing was filed anywhere').toBe(false);
  });

  it('files a key under the provider it was typed for', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.provider = 'gemini';
    await flush();

    s.apiKeyDraft.value = 'AIza-gemini';
    await s.saveSettings();

    const filed = calls.find(c => c.cmd === 'syn_set_api_key');
    expect(filed?.args).toEqual({ provider: 'gemini', key: 'AIza-gemini' });
  });

  /** Ollama runs here and has nothing to authenticate; nothing is filed. */
  it('files nothing for a provider that takes no key', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.provider = 'ollama';
    await flush();

    s.apiKeyDraft.value = 'typed anyway';
    await s.saveSettings();
    expect(calls.some(c => c.cmd === 'syn_set_api_key')).toBe(false);
  });
});

describe('Gemini in the settings screen', () => {
  it('is offered', () => {
    expect(settingsPanel).toContain('<option value="gemini">');
    for (const locale of [en, vi_]) {
      expect(locale.syn).toHaveProperty('provider_gemini');
      expect(locale.syn).toHaveProperty('provider_gemini_desc');
      expect(locale.syn).toHaveProperty('api_key_desc_gemini');
    }
  });

  /** Gemini has one address. A base-URL field would be a thing to get wrong
   *  with nothing it could usefully be set to. */
  it('asks for a key and not for an address', () => {
    expect(settingsPanel).toContain('<div v-if="!usingGemini">');
  });
});

/**
 * The key field is shaped like the key that goes in it.
 *
 * It said `sk-…` under Google Gemini. Gemini keys start with `AIza`; a hint
 * shaped like an OpenAI key is a hint that an OpenAI key goes here.
 */
describe('what an empty key field looks like', () => {
  it('looks like a Gemini key under Gemini', () => {
    expect(en.syn.api_key_placeholder_gemini.startsWith('AIza')).toBe(true);
    expect(vi_.syn.api_key_placeholder_gemini.startsWith('AIza')).toBe(true);
    expect(settingsPanel).toContain("usingGemini.value ? t('syn.api_key_placeholder_gemini')");
  });
});

/**
 * A model name only means something to the provider that listed it.
 *
 * Switching the selector to Gemini left "gpt-5.6-luna" as the default model,
 * listed under a heading that now read Gemini. Saved like that, the first
 * message asks Gemini for an OpenAI model and comes back a 404 — after the
 * person has done everything the screen asked of them.
 */
describe('the default model across a change of provider', () => {
  it('does not show another provider’s models under this one', () => {
    expect(settingsPanel).toContain('v-if="modelsAreStale"');
    for (const locale of [en, vi_]) {
      expect(locale.syn).toHaveProperty('default_model_after_save');
    }
  });

  it('forgets a default chosen from another provider’s list when saved', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.default_model = 'gpt-5.6-luna';

    s.settings.value.provider = 'gemini';
    await flush();
    await s.saveSettings();

    const saved = calls.find(c => c.cmd === 'syn_save_settings');
    expect((saved?.args.settings as { default_model: unknown }).default_model).toBeNull();
    expect(s.savedProvider.value).toBe('gemini');
  });

  it('keeps it when the provider did not change', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.default_model = 'gpt-5.6-luna';
    await s.saveSettings();

    const saved = calls.find(c => c.cmd === 'syn_save_settings');
    expect((saved?.args.settings as { default_model: unknown }).default_model).toBe('gpt-5.6-luna');
  });

  /** And switching away and back again is not a change at all. */
  it('keeps it when the selector comes back to where it started', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.default_model = 'gpt-5.6-luna';

    s.settings.value.provider = 'gemini';
    await flush();
    s.settings.value.provider = 'open_ai_compat';
    await flush();
    await s.saveSettings();

    const saved = calls.find(c => c.cmd === 'syn_save_settings');
    expect((saved?.args.settings as { default_model: unknown }).default_model).toBe('gpt-5.6-luna');
  });
});
