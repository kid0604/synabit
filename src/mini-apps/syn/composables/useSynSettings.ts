import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';

export interface SynSettings {
  // Connection
  ollama_url: string;
  default_model: string | null;

  // Generation
  temperature: number;
  max_tool_iterations: number;

  // RAG
  rag_enabled: boolean;
  max_context_chars: number;
  include_finance: boolean;
  include_feeds: boolean;
  graph_expansion_depth: number;

  // Personality
  personality: string;
  custom_system_prompt: string | null;
}

const DEFAULT_SETTINGS: SynSettings = {
  ollama_url: 'http://localhost:11434',
  default_model: null,
  temperature: 0.7,
  max_tool_iterations: 5,
  rag_enabled: true,
  max_context_chars: 32000,
  include_finance: true,
  include_feeds: true,
  graph_expansion_depth: 1,
  personality: 'auto',
  custom_system_prompt: null,
};

export function useSynSettings(vaultPath: string) {
  const settings = ref<SynSettings>({ ...DEFAULT_SETTINGS });
  const isLoading = ref(false);
  const isSaving = ref(false);

  const loadSettings = async () => {
    isLoading.value = true;
    try {
      const result = await invoke<SynSettings>('syn_get_settings', { vaultPath });
      settings.value = result;
    } catch (e) {
      logger.error('[Syn] Failed to load settings', e);
      settings.value = { ...DEFAULT_SETTINGS };
    } finally {
      isLoading.value = false;
    }
  };

  const saveSettings = async () => {
    isSaving.value = true;
    try {
      await invoke('syn_save_settings', { vaultPath, settings: settings.value });
    } catch (e) {
      logger.error('[Syn] Failed to save settings', e);
    } finally {
      isSaving.value = false;
    }
  };

  const resetToDefaults = () => {
    settings.value = { ...DEFAULT_SETTINGS };
  };

  return {
    settings,
    isLoading,
    isSaving,
    loadSettings,
    saveSettings,
    resetToDefaults,
  };
}
