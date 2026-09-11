import { describe, it, expect, vi, beforeEach } from 'vitest';
import { nextTick } from 'vue';

const calls: Array<{ cmd: string; args: Record<string, unknown> }> = [];
/** What `Syn/settings.json` says, as far as these tests need it to. */
let file: Record<string, unknown> = { provider: 'open_ai_compat' };
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: Record<string, unknown> = {}) => {
    calls.push({ cmd, args });
    if (cmd === 'syn_get_settings') return file;
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

beforeEach(() => {
  calls.length = 0;
  file = { provider: 'open_ai_compat' };
});

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
   * One field on screen, a draft behind it per provider.
   *
   * A key typed under OpenAI must never be filed as Gemini's. And a Gemini key
   * typed while setting Gemini up as a second option must survive switching
   * back to OpenAI — which is exactly how somebody adds a second provider
   * without leaving the first.
   */
  it('keeps what was typed with the provider it was typed for', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();

    s.apiKeyDraft.value = 'sk-openai-xxxx';
    s.settings.value.provider = 'gemini';
    await flush();
    expect(s.apiKeyDraft.value, 'Gemini’s field is its own').toBe('');

    s.apiKeyDraft.value = 'AIza-gemini';
    s.settings.value.provider = 'open_ai_compat';
    await flush();
    expect(s.apiKeyDraft.value, 'and OpenAI’s draft is where it was left').toBe('sk-openai-xxxx');
  });

  /** Set Gemini up, go back to OpenAI, save: Gemini's key is stored, and
   *  OpenAI is still the provider in use. */
  it('files every draft under its own slot, whichever provider is selected', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();

    s.settings.value.provider = 'gemini';
    await flush();
    s.apiKeyDraft.value = 'AIza-gemini';
    s.settings.value.provider = 'open_ai_compat';
    await flush();
    await s.saveSettings();

    const filed = calls.filter(c => c.cmd === 'syn_set_api_key').map(c => c.args);
    expect(filed).toEqual([{ provider: 'gemini', key: 'AIza-gemini' }]);
    const saved = calls.find(c => c.cmd === 'syn_save_settings');
    expect((saved?.args.settings as { provider: string }).provider).toBe('open_ai_compat');
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

  /**
   * The flow that was broken: OpenAI in use with its own default, the selector
   * moved to Gemini to set it up as a second option, and moved back. Coming
   * back must find `gpt-5.6-luna` exactly where it was.
   */
  it('gives each provider its own default, and loses neither across a switch', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.default_model = 'gpt-5.6-luna';

    s.settings.value.provider = 'gemini';
    await flush();
    expect(s.settings.value.default_model, 'Gemini is not handed an OpenAI model').toBeNull();

    s.settings.value.default_model = 'gemini-3.8-flash';
    s.settings.value.provider = 'open_ai_compat';
    await flush();
    expect(s.settings.value.default_model).toBe('gpt-5.6-luna');

    s.settings.value.provider = 'gemini';
    await flush();
    expect(s.settings.value.default_model).toBe('gemini-3.8-flash');
  });

  it('saves every provider’s default, with the active one in use', async () => {
    const s = useSynSettings('/vault');
    await s.loadSettings();
    s.settings.value.default_model = 'gpt-5.6-luna';
    s.settings.value.provider = 'gemini';
    await flush();
    s.settings.value.default_model = 'gemini-3.8-flash';
    await s.saveSettings();

    const saved = calls.find(c => c.cmd === 'syn_save_settings')?.args.settings as {
      provider: string; default_model: string | null; default_models: Record<string, string>;
    };
    expect(saved.provider).toBe('gemini');
    expect(saved.default_model).toBe('gemini-3.8-flash');
    expect(saved.default_models).toEqual({
      open_ai_compat: 'gpt-5.6-luna',
      gemini: 'gemini-3.8-flash',
    });
    expect(s.savedProvider.value).toBe('gemini');
  });

  /** A file written before providers kept their own defaults has one, and it
   *  belongs to the provider that file names. Loading must not file it under
   *  whatever the empty form happened to start on. */
  it('files an old single default under the provider the file names', async () => {
    file = { provider: 'open_ai_compat', default_model: 'gpt-5.6-luna' };
    const s = useSynSettings('/vault');
    await s.loadSettings();
    await flush();

    expect(s.settings.value.default_models).toEqual({ open_ai_compat: 'gpt-5.6-luna' });
    expect(s.settings.value.default_model, 'and it is still the one in use').toBe('gpt-5.6-luna');
    expect(
      s.settings.value.default_models,
      'nothing filed under the Ollama the empty form started on',
    ).not.toHaveProperty('ollama');
  });
});
