/**
 * Reading what Syn actually did, and what it was actually told.
 *
 * Both halves answer the same question from opposite ends. A transcript says
 * what happened; the prompt preview says what Syn knew going in. Until now
 * neither existed: when Syn did the wrong thing there was `log::info!` on the
 * user's own machine and nothing else, and the system prompt was a string
 * built in Rust that nobody outside the debugger had ever seen.
 *
 * Deliberately thin. There is no caching and no polling here — a panel that is
 * open reloads when it is asked to, and a panel that is closed should not be
 * reading the disk.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import type { Run, RunSummary, PromptPreview } from '../types';

export function useSynRuns(vaultPath: () => string) {
  const runs = ref<RunSummary[]>([]);
  const selected = ref<Run | null>(null);
  const preview = ref<PromptPreview | null>(null);
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

  const loadRuns = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      runs.value = await invoke<RunSummary[]>('syn_list_runs', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error('[Syn] Failed to list runs', e);
      error.value = asMessage(e);
      runs.value = [];
    } finally {
      isLoading.value = false;
    }
  };

  const openRun = async (runId: string) => {
    error.value = null;
    try {
      selected.value = await invoke<Run>('syn_get_run', { vaultPath: vaultPath(), runId });
    } catch (e) {
      logger.error('[Syn] Failed to read run', e);
      error.value = asMessage(e);
      selected.value = null;
    }
  };

  const cancelRun = async (runId: string) => {
    try {
      await invoke('syn_cancel_run', { runId });
      // The run notices between steps, so what it says now is not yet what it
      // will say. Reload rather than guess.
      await loadRuns();
    } catch (e) {
      logger.error('[Syn] Failed to cancel run', e);
      error.value = asMessage(e);
    }
  };

  const deleteRun = async (runId: string) => {
    try {
      await invoke('syn_delete_run', { vaultPath: vaultPath(), runId });
      if (selected.value?.id === runId) selected.value = null;
      await loadRuns();
    } catch (e) {
      logger.error('[Syn] Failed to delete run', e);
      error.value = asMessage(e);
    }
  };

  /**
   * The prompt this vault would send.
   *
   * `message` is optional and worth giving: without one the preview is the
   * fixed part, and with one it includes the context that question would pull
   * in — which is the only way to see how much of the window retrieval takes.
   */
  const loadPreview = async (message?: string) => {
    isLoading.value = true;
    error.value = null;
    try {
      preview.value = await invoke<PromptPreview>('syn_preview_prompt', {
        vaultPath: vaultPath(),
        message: message?.trim() ? message : undefined,
      });
    } catch (e) {
      logger.error('[Syn] Failed to preview the prompt', e);
      error.value = asMessage(e);
      preview.value = null;
    } finally {
      isLoading.value = false;
    }
  };

  return { runs, selected, preview, isLoading, error, loadRuns, openRun, cancelRun, deleteRun, loadPreview };
}
