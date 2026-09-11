import { ref, watch } from 'vue';
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
  default_model: string | null;

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

  /** What the user typed into the key field this session. */
  const apiKeyDraft = ref('');

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
   * Switching provider throws away a half-typed key.
   *
   * The field is one field for whichever provider is selected. Type an OpenAI
   * key, change the selector to Gemini, press Save — and without this the
   * OpenAI key is filed as Gemini's, where it fails with a message about a key
   * the person is sure they entered correctly. They did; into the other slot.
   */
  watch(() => settings.value.provider, () => {
    apiKeyDraft.value = '';
    void refreshApiKeyState();
  });

  const loadSettings = async () => {
    isLoading.value = true;
    try {
      const result = await invoke<SynSettings>('syn_get_settings', { vaultPath });
      // Merged over the defaults rather than assigned: a settings file written
      // before providers existed has neither `provider` nor `openai_base_url`,
      // and binding a `<select>` to `undefined` leaves it blank.
      settings.value = { ...DEFAULT_SETTINGS, ...result };
    } catch (e) {
      logger.error('[Syn] Failed to load settings', e);
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
      // A default model chosen from another provider's list goes with it.
      //
      // `gpt-5.6-luna` saved as Gemini's default is sent to Gemini on the first
      // message and comes back a 404 — after the person has already done
      // everything the screen asked. Emptied, the chat picks one of the new
      // provider's own models, and this screen offers them next time.
      if (settings.value.provider !== savedProvider.value) {
        settings.value.default_model = null;
      }
      await invoke('syn_save_settings', { vaultPath, settings: settings.value });
      savedProvider.value = settings.value.provider;

      // Only when the user typed something. An untouched field must not clear
      // a key that is already stored.
      if (apiKeyDraft.value.trim() && takesKey(settings.value.provider)) {
        await invoke('syn_set_api_key', {
          provider: settings.value.provider,
          key: apiKeyDraft.value.trim(),
        });
        apiKeyDraft.value = '';
        await refreshApiKeyState();
      }
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
