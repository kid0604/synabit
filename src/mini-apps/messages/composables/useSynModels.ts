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
  const pullError = ref<string | null>(null);

  // Polling: auto-detect Ollama when disconnected
  const isPolling = ref(false);
  let pollTimer: ReturnType<typeof setInterval> | null = null;

  // Health check: periodic check when connected
  let healthTimer: ReturnType<typeof setInterval> | null = null;

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

  /**
   * Models that cannot hold a conversation, by the only signal available.
   *
   * A name, because the OpenAI list endpoint says nothing about what a model
   * is for — every entry is `{id, object: "model", owned_by}`. So this is a
   * heuristic and is used only to break a tie when nothing has been chosen:
   * anything it gets wrong is still one click away in the picker, and the
   * vault's own `default_model` outranks it entirely.
   */
  const isEmbeddingModel = (name: string): boolean =>
    /embedding|embed-|moderation|whisper|tts|dall-e|audio|image/i.test(name);

  const fetchModels = async (vaultPath?: string): Promise<ModelInfo[]> => {
    loadingModels.value = true;
    try {
      const result = await invoke<ModelInfo[]>('syn_list_models',
        vaultPath ? { vaultPath } : undefined
      );
      models.value = result;

      // The vault's own default first, then whatever is available.
      //
      // Taking `result[0]` was wrong in a way that only shows on a provider
      // with a large catalogue: against Ollama the list is a handful of chat
      // models and any of them works, but an OpenAI-compatible endpoint
      // returns everything the account can reach, and the first entry was
      // `text-embedding-ada-002`. The header said Syn was running an
      // embedding model, and every message sent to it would have failed —
      // while `Syn/settings.json` said `gpt-5.6-luna` the whole time.
      //
      // The fallback stays, because a vault that has never chosen one has to
      // start somewhere, but it now prefers a model that can hold a
      // conversation over one that cannot.
      if (!selectedModel.value && result.length > 0) {
        const preferred = vaultPath
          ? await invoke<{ default_model: string | null }>('syn_get_settings', { vaultPath })
              .then(s => s.default_model)
              .catch(() => null)
          : null;

        const available = (name: string | null) =>
          !!name && result.some(m => m.name === name);

        selectedModel.value = available(preferred)
          ? (preferred as string)
          : (result.find(m => !isEmbeddingModel(m.name)) ?? result[0]).name;
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

  const pullModel = async (name: string, vaultPath?: string): Promise<boolean> => {
    pullingModel.value = true;
    pullProgress.value = 0;
    pullStatus.value = '';
    pullError.value = null;

    let unlisten: UnlistenFn | null = null;

    try {
      // Listen for progress events
      unlisten = await listen<{ model: string; status: string; completed?: number; total?: number }>(
        'syn-pull-progress',
        (event) => {
          const data = event.payload;
          if (data.model === name) {
            pullProgress.value = (data.completed && data.total)
              ? (data.completed / data.total) * 100
              : 0;
            pullStatus.value = data.status;
          }
        }
      );

      await invoke('syn_pull_model', {
        modelName: name,
        ...(vaultPath ? { vaultPath } : {}),
      });

      // Refresh models list after pulling
      await fetchModels(vaultPath);

      // Auto-select the newly pulled model
      if (!selectedModel.value) {
        selectedModel.value = name;
      }

      return true;
    } catch (e) {
      logger.error('[Syn] Failed to pull model', e);
      pullError.value = String(e);
      return false;
    } finally {
      pullingModel.value = false;
      pullProgress.value = 0;
      pullStatus.value = '';
      if (unlisten) unlisten();
    }
  };

  const deleteModel = async (name: string, vaultPath?: string): Promise<boolean> => {
    try {
      await invoke('syn_delete_model', {
        modelName: name,
        ...(vaultPath ? { vaultPath } : {}),
      });

      // If we deleted the selected model, clear selection
      if (selectedModel.value === name) {
        selectedModel.value = '';
      }

      // Refresh models list
      await fetchModels(vaultPath);
      return true;
    } catch (e) {
      logger.error('[Syn] Failed to delete model', e);
      return false;
    }
  };

  // ── Polling: auto-detect Ollama when disconnected ──────────

  const startPolling = (vaultPath?: string, attempt = 0) => {
    if (pollTimer) return; // already polling
    isPolling.value = true;
    
    const backoff = Math.min(5000 * Math.pow(1.5, attempt), 60000);
    pollTimer = setTimeout(async () => {
      pollTimer = null;
      const result = await checkStatus(vaultPath);
      if (result.connected) {
        isPolling.value = false;
        await fetchModels(vaultPath);
      } else {
        startPolling(vaultPath, attempt + 1);
      }
    }, backoff);
  };

  const stopPolling = () => {
    if (pollTimer) {
      clearTimeout(pollTimer);
      pollTimer = null;
    }
    isPolling.value = false;
  };

  // ── Health check: periodic when connected ──────────────────

  const startHealthCheck = (vaultPath?: string) => {
    if (healthTimer) return;
    healthTimer = setInterval(async () => {
      const result = await checkStatus(vaultPath);
      if (!result.connected) {
        stopHealthCheck();
      }
    }, 30000); // every 30s
  };

  const stopHealthCheck = () => {
    if (healthTimer) {
      clearInterval(healthTimer);
      healthTimer = null;
    }
  };

  const cleanup = () => {
    stopPolling();
    stopHealthCheck();
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
    pullError,
    isPolling,
    checkStatus,
    fetchModels,
    pullModel,
    deleteModel,
    formatModelSize,
    startPolling,
    stopPolling,
    startHealthCheck,
    stopHealthCheck,
    cleanup,
  };
}
