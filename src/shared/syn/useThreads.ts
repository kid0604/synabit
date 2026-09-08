/**
 * The work that is open between the user and Syn.
 *
 * # Why there is a composable here and not a store
 *
 * A thread is a node. It is written by the same path everything else in the
 * vault is written by, it is found by Nexus, it appears in Things, and it syncs
 * — so there is nothing to keep in memory between screens that the vault is not
 * already keeping. This is a list and three calls.
 *
 * # Why the list is short, and stays short
 *
 * `open` filters out closed threads because the picker is a thing people reach
 * for mid-sentence, and a picker that grows forever stops being reached for.
 * Closed threads are not hidden anywhere — they are nodes, so Nexus finds them
 * and Things lists them, which is the whole reason they could be left out of
 * here without being lost.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';

/** Whose move it is. Mirrors `syn::thread::State`. */
export type ThreadState = 'mine' | 'yours' | 'world' | 'resting' | 'closed';

export interface Thread {
  /** Vault-relative path, which is also how every node tool addresses it. */
  id: string;
  title: string;
  body: string;
  state: ThreadState;
  waiting_for?: string | null;
  /** From the node's own dates — not stored twice. See `syn/thread.rs`. */
  opened: string;
  last_moved: string;
}

/** Every state, in the order a picker should offer them. */
export const THREAD_STATES: ThreadState[] = ['mine', 'yours', 'world', 'resting', 'closed'];

/** Still open work, most recently moved first. */
export function openThreads(threads: Thread[]): Thread[] {
  return threads.filter((t) => t.state !== 'closed');
}

/** What the runs say about a thread. Mirrors `syn::thread::Usage`. */
export interface ThreadUsage {
  /** Runs whose prompt carried this thread. */
  runs: number;
  /** Of those, runs that wrote something back into it. */
  wrote_back: number;
}

/** The counts, per thread and in total. Mirrors `syn::thread::Stats`. */
export interface ThreadStats {
  per_thread: Record<string, ThreadUsage>;
  runs_total: number;
  runs_in_a_thread: number;
  runs_that_wrote_back: number;
}

/**
 * How often Syn was standing on something, across the runs still on disk.
 *
 * Mirrors `syn::footing::Tally`. `unmeasured` is separate from `guessing` on
 * purpose — runs written before footings existed were not guesses, nobody
 * looked — and the screen must never add the two.
 */
export interface FootingTally {
  measured: number;
  grounded: number;
  inferred: number;
  guessing: number;
  unmeasured: number;
}

export function useThreads(vaultPath: () => string) {
  const threads = ref<Thread[]>([]);
  /**
   * Whether any of this is used.
   *
   * Loaded beside the threads because the question it answers is about them:
   * a thread reaches the model by riding in the prompt, and being told to write
   * back is not the same as doing it. See `syn::thread::usage`.
   */
  const stats = ref<ThreadStats | null>(null);
  /**
   * How often Syn was guessing.
   *
   * Loaded here rather than in a composable of its own because it is read off
   * the same runs, on the same screen, at the same moment — and because the
   * failure this is meant to break is a number that gets collected and never
   * shown. Another loader nobody calls would be that failure again.
   */
  const footing = ref<FootingTally | null>(null);
  const error = ref<string | null>(null);
  const isLoading = ref(false);

  const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      threads.value = await invoke<Thread[]>('syn_list_threads');
      stats.value = await invoke<ThreadStats>('syn_thread_usage', { vaultPath: vaultPath() });
      footing.value = await invoke<FootingTally>('syn_footing_tally', {
        vaultPath: vaultPath(),
      });
    } catch (e) {
      logger.error('[Syn] Could not read the threads', e);
      error.value = asMessage(e);
    } finally {
      isLoading.value = false;
    }
  };

  /**
   * Start one, and answer with its id so the caller can switch to it.
   *
   * `null` on failure rather than throwing: the caller is a bar in the middle
   * of somebody's sentence, and the right response to a thread that could not
   * be made is to say so and let them keep typing.
   */
  const open = async (title: string): Promise<string | null> => {
    error.value = null;
    try {
      const id = await invoke<string>('syn_open_thread', { vaultPath: vaultPath(), title });
      await load();
      return id;
    } catch (e) {
      logger.error('[Syn] Could not start a thread', e);
      error.value = asMessage(e);
      return null;
    }
  };

  /** Move it: whose turn it is, and what it is waiting for. */
  const move = async (id: string, state: ThreadState, waitingFor?: string) => {
    error.value = null;
    try {
      await invoke('syn_move_thread', {
        vaultPath: vaultPath(),
        id,
        threadState: state,
        waitingFor: waitingFor?.trim() ? waitingFor.trim() : undefined,
      });
      await load();
      return true;
    } catch (e) {
      logger.error('[Syn] Could not move the thread', e);
      error.value = asMessage(e);
      return false;
    }
  };

  return { threads, stats, footing, error, isLoading, load, open, move };
}
