import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { OllamaStatus, ModelInfo } from '../types';

export function useSynModels() {
  const models = ref<ModelInfo[]>([]);
  const status = ref<OllamaStatus>({
    connected: false,
    version: null,
    url: 'http://localhost:11434',
  });
  const selectedModel = ref<string>('');
  const loadingModels = ref(false);
  const pullingModel = ref(false);
  const pullProgress = ref(0);
  const pullStatus = ref('');

  const checkStatus = async (vaultPath?: string): Promise<OllamaStatus> => {
    try {
      const result = await invoke<OllamaStatus>('syn_check_status', vaultPath ? { vaultPath } : undefined);
      status.value = result;
      return result;
    } catch (e) {
      logger.error('[Syn] Failed to check Ollama status', e);
      status.value = { connected: false, version: null, url: status.value.url };
      return status.value;
    }
  };

  const fetchModels = async (): Promise<ModelInfo[]> => {
    loadingModels.value = true;
    try {
      const result = await invoke<ModelInfo[]>('syn_list_models');
      models.value = result;

      // Auto-select first model if none selected
      if (!selectedModel.value && result.length > 0) {
        selectedModel.value = result[0].name;
      }

      return result;
    } catch (e) {
      logger.error('[Syn] Failed to fetch models', e);
      models.value = [];
      return [];
    } finally {
      loadingModels.value = false;
    }
  };

  const pullModel = async (name: string): Promise<boolean> => {
    pullingModel.value = true;
    pullProgress.value = 0;
    pullStatus.value = '';

    let unlisten: UnlistenFn | null = null;

    try {
      // Listen for progress events
      unlisten = await listen<{ model: string; status: string; progress: number }>(
        'syn-pull-progress',
        (event) => {
          const data = event.payload;
          if (data.model === name) {
            pullProgress.value = data.progress;
            pullStatus.value = data.status;
          }
        }
      );

      await invoke('syn_pull_model', { name });

      // Refresh models list after pulling
      await fetchModels();

      // Auto-select the newly pulled model
      if (!selectedModel.value) {
        selectedModel.value = name;
      }

      return true;
    } catch (e) {
      logger.error('[Syn] Failed to pull model', e);
      return false;
    } finally {
      pullingModel.value = false;
      pullProgress.value = 0;
      pullStatus.value = '';
      if (unlisten) unlisten();
    }
  };

  const deleteModel = async (name: string): Promise<boolean> => {
    try {
      await invoke('syn_delete_model', { name });

      // If we deleted the selected model, clear selection
      if (selectedModel.value === name) {
        selectedModel.value = '';
      }

      // Refresh models list
      await fetchModels();
      return true;
    } catch (e) {
      logger.error('[Syn] Failed to delete model', e);
      return false;
    }
  };

  const formatModelSize = (bytes: number): string => {
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = bytes / (1024 * 1024);
    return `${mb.toFixed(0)} MB`;
  };

  return {
    models,
    status,
    selectedModel,
    loadingModels,
    pullingModel,
    pullProgress,
    pullStatus,
    checkStatus,
    fetchModels,
    pullModel,
    deleteModel,
    formatModelSize,
  };
}
