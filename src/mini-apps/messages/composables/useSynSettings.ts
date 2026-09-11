import { ref, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';

/**
 * Which service Syn talks to.
 *
 * The strings are what `SynProvider` serialises to in Rust and what ends up in
 * `{vault}/Syn/settings.json`; a Rust test pins them, because a typo here
 * would be a settings file that silently loads as Ollama.
 */
export type SynProviderId = 'ollama' | 'open_ai_compat' | 'gemini';

/**
 * The providers that take an API key, each filed under its own slot.
 *
 * Ollama is not here: it runs on this machine and has nothing to authenticate.
 */
export const KEYED_PROVIDERS: readonly SynProviderId[] = ['open_ai_compat', 'gemini'];

/** Whether a provider is one a key is stored for. */
export const takesKey = (provider: SynProviderId): boolean => KEYED_PROVIDERS.includes(provider);

export interface SynSettings {
  /**
   * The switch that turns Syn off.
   *
   * Off means no message is sent, no run is driven, nothing is reflected on,
   * no memory reaches a prompt, and the ask bar does not open anywhere. It
   * deliberately leaves the app's own reminders alone — those are the calendar
   * speaking as *Synabit System*, not Syn — and it leaves threads alone,
   * because they are ordinary vault nodes and Things still lists them.
   *
   * The backend refuses on its own when this is false; the screen hiding the
   * composer is the courtesy, not the enforcement.
   */
  enabled: boolean;

  // Connection
  provider: SynProviderId;
  ollama_url: string;
  openai_base_url: string;
  /**
   * `reasoning_effort` to pin, or null to let the backend work it out.
   *
   * Deliberately has no control in Settings. Reasoning models refuse function
   * tools on /chat/completions unless this is 'none', and every other server
   * speaking this API rejects a request carrying a field it does not know — so
   * neither value is a safe default and no user can be expected to know which
   * they need. The backend learns it from the first refusal instead.
   *
   * Kept in the type so that a value hand-written into
   * `{vault}/Syn/settings.json` survives a save from this screen rather than
   * being quietly dropped.
   */
  openai_reasoning_effort: string | null;
  /** The default model for the provider in use. What everything else reads. */
  default_model: string | null;
  /**
   * Each provider's own default, kept while it is not the one in use.
   *
   * A model name belongs to the provider that listed it. Switching the selector
   * to Gemini to paste its key, and back to OpenAI, must neither send
   * `gpt-5.6-luna` to Gemini nor come back to find it gone.
   */
  default_models: Partial<Record<SynProviderId, string>>;

  // Generation
  temperature: number;
  max_tool_iterations: number;

  // RAG
  rag_enabled: boolean;
  max_context_chars: number;
  /**
   * How much of a web page reaches the model at once, in characters.
   *
   * `null` means "let the provider decide", which is not the same as any
   * particular number — a model on somebody's laptop and a hosted one want
   * genuinely different answers. See `syn::web::page_chars`.
   */
  max_page_chars: number | null;
  include_finance: boolean;
  include_feeds: boolean;
  graph_expansion_depth: number;

  // Personality
  custom_system_prompt: string | null;

  // Context limits
  num_ctx: number;
  max_history_messages: number;
  /**
   * Whether Syn looks back at each exchange and proposes what to remember.
   *
   * One extra completion per answered message — no tools, a small prompt.
   * Proposals go to a tray for review; nothing is written to the vault without
   * being accepted.
   */
  memory_reflection: boolean;
}

/**
 * What Reset restores, and what is used when settings cannot be read.
 *
 * These must match `SynSettings::default()` in Rust. They did not: this held
 * `num_ctx: 4096`, `max_context_chars: 32000` and `max_history_messages: 20`
 * against the backend's 8192, 12000 and 50, so pressing Reset gave a
 * configuration no fresh vault has ever had.
 */
const DEFAULT_SETTINGS: SynSettings = {
  enabled: true,
  provider: 'ollama',
  ollama_url: 'http://localhost:11434',
  openai_base_url: 'https://api.openai.com/v1',
  openai_reasoning_effort: null,
  default_model: null,
  default_models: {},
  temperature: 0.7,
  max_tool_iterations: 12,
  rag_enabled: true,
  max_context_chars: 12000,
  max_page_chars: null,
  include_finance: true,
  include_feeds: true,
  graph_expansion_depth: 1,
  custom_system_prompt: null,
  num_ctx: 8192,
  max_history_messages: 50,
  memory_reflection: true,
};

export function useSynSettings(vaultPath: string) {
  const settings = ref<SynSettings>({ ...DEFAULT_SETTINGS });
  const isLoading = ref(false);
  const isSaving = ref(false);

  /**
   * Whether a key is stored for the provider that is selected.
   *
   * Only ever a boolean. The key itself lives in the OS keychain and there is
   * no command that reads one back — the UI needs to know that one is set, not
   * what it is, and a key that can be read is a key that can leak into a log,
   * a screenshot or a bug report.
   *
   * Per provider, because each has its own slot. It used to be hard-wired to
   * the OpenAI-compatible one, which was right while that was the only
   * provider with a key.
   */
  const hasApiKey = ref(false);

  /**
   * What was typed into the key field, per provider.
   *
   * One field on screen, a draft behind it for each provider. The field shows
   * the selected provider's, so a key typed under OpenAI can never be filed as
   * Gemini's — and a Gemini key typed while setting Gemini up as a second
   * option survives switching back to OpenAI and is saved to Gemini's slot.
   */
  const drafts = ref<Partial<Record<SynProviderId, string>>>({});

  /** The draft for whichever provider is selected — what the field binds to. */
  const apiKeyDraft = computed<string>({
    get: () => drafts.value[settings.value.provider] ?? '',
    set: (typed) => { drafts.value = { ...drafts.value, [settings.value.provider]: typed }; },
  });

  /**
   * The provider the rest of the app is using — what was loaded or last saved.
   *
   * Not the same as `settings.provider` while the form is being edited, and the
   * difference matters: the model list on this screen came from *this* one. A
   * model name only means something to the provider that listed it, so a list
   * fetched from OpenAI shown under a selector reading "Gemini" is a list of
   * names that would all be 404s.
   */
  const savedProvider = ref<SynProviderId>(DEFAULT_SETTINGS.provider);

  const refreshApiKeyState = async () => {
    const provider = settings.value.provider;
    if (!takesKey(provider)) {
      hasApiKey.value = false;
      return;
    }
    try {
      hasApiKey.value = await invoke<boolean>('syn_has_api_key', { provider });
    } catch (e) {
      logger.error('[Syn] Failed to check for a stored API key', e);
      hasApiKey.value = false;
    }
  };

  /**
   * Which provider the form was last showing, so a switch knows where from.
   *
   * Set synchronously on load, so the watcher below sees a load as no switch
   * at all — otherwise loading the file would file its own default under
   * whatever provider the empty form started on.
   */
  let showing: SynProviderId = DEFAULT_SETTINGS.provider;

  /**
   * Moving the selector puts one provider's default model away and gets the
   * other's out.
   *
   * `default_model` is the one field everything reads, so it always holds the
   * selected provider's model: never a name left over from another provider,
   * and never a choice lost by passing through one.
   */
  watch(() => settings.value.provider, (now) => {
    if (now !== showing) {
      const models = { ...settings.value.default_models };
      if (settings.value.default_model) models[showing] = settings.value.default_model;
      settings.value.default_models = models;
      settings.value.default_model = models[now] ?? null;
      showing = now;
    }
    void refreshApiKeyState();
  });

  const loadSettings = async () => {
    isLoading.value = true;
    try {
      const result = await invoke<SynSettings>('syn_get_settings', { vaultPath });
      // Merged over the defaults rather than assigned: a settings file written
      // before providers existed has neither `provider` nor `openai_base_url`,
      // and binding a `<select>` to `undefined` leaves it blank.
      const loaded = { ...DEFAULT_SETTINGS, ...result };
      // A file written before providers kept their own defaults has one
      // `default_model`, and it belongs to the provider that file names.
      loaded.default_models = { ...loaded.default_models };
      if (loaded.default_model && !loaded.default_models[loaded.provider]) {
        loaded.default_models[loaded.provider] = loaded.default_model;
      }
      showing = loaded.provider;
      settings.value = loaded;
    } catch (e) {
      logger.error('[Syn] Failed to load settings', e);
      showing = DEFAULT_SETTINGS.provider;
      settings.value = { ...DEFAULT_SETTINGS };
    } finally {
      savedProvider.value = settings.value.provider;
      isLoading.value = false;
    }
    await refreshApiKeyState();
  };

  const saveSettings = async () => {
    isSaving.value = true;
    try {
      // The active provider's choice goes in its own slot as well, so it is
      // there to come back to after using another one.
      const models = { ...settings.value.default_models };
      const active = settings.value.provider;
      if (settings.value.default_model) models[active] = settings.value.default_model;
      else delete models[active];
      settings.value.default_models = models;

      await invoke('syn_save_settings', { vaultPath, settings: settings.value });
      savedProvider.value = settings.value.provider;

      // Every draft to its own provider's slot, whichever one is selected.
      // Only what was typed: an untouched field must not clear a stored key.
      for (const provider of KEYED_PROVIDERS) {
        const typed = drafts.value[provider]?.trim();
        if (!typed) continue;
        await invoke('syn_set_api_key', { provider, key: typed });
      }
      drafts.value = {};
      await refreshApiKeyState();
    } catch (e) {
      logger.error('[Syn] Failed to save settings', e);
    } finally {
      isSaving.value = false;
    }
  };

  /** Forget the stored key. This is how a user revokes one. */
  const clearApiKey = async () => {
    try {
      if (!takesKey(settings.value.provider)) return;
      await invoke('syn_set_api_key', { provider: settings.value.provider, key: '' });
      apiKeyDraft.value = '';
      await refreshApiKeyState();
    } catch (e) {
      logger.error('[Syn] Failed to clear the API key', e);
    }
  };

  /**
   * Restore the shipped defaults.
   *
   * Settings only — the stored API key is untouched, because "reset my
   * preferences" and "revoke my credential" are different requests and the
   * second one has its own button.
   */
  const resetToDefaults = () => {
    settings.value = { ...DEFAULT_SETTINGS };
  };

  return {
    settings,
    savedProvider,
    isLoading,
    isSaving,
    hasApiKey,
    apiKeyDraft,
    loadSettings,
    saveSettings,
    clearApiKey,
    resetToDefaults,
  };
}
