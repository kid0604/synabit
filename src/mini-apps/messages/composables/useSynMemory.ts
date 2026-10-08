/**
 * Reading and correcting what Syn remembers.
 *
 * Reads go through a typed command, because a memory has a dozen frontmatter
 * keys with defaults and clamping and a screen that re-derived those would be a
 * second opinion about what a memory is. Writes go the other way, through the
 * ordinary node service — a memory is an ordinary node, and that path already
 * has version history, sync and a trash behind it. Nothing here needs to know
 * that; it needs only to not build a second one.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';
import type { Memory, MemoryBudget, Proposal } from '../types';
import { errorText } from '../../../shared/errorText';
import { todayIso } from '../../../shared/localDay';

/**
 * Today, as the `YYYY-MM-DD` string every date on a memory is written in — the
 * reader's day, not UTC's, or a memory confirmed before seven in the morning
 * in Hà Nội is confirmed yesterday.
 */
export const today = () => todayIso();

/**
 * Past its own review date, so worth asking about again.
 *
 * `review_after` is set by whoever wrote the memory — it is a memory saying "I
 * may go out of date, check me" — and until this reached a screen it was a
 * reminder nobody received.
 */
export const isStale = (memory: Memory, now = today()) =>
  !!memory.review_after && memory.review_after < now;

/**
 * The order the memory list is read in.
 *
 * Anything asking to be checked first, because it is the only thing on that
 * screen wanting an answer rather than a read. Then pinned, then most recently
 * confirmed — which is the same order the prompt gives memories up in, so the
 * screen reads in the order Syn would forget.
 *
 * A pure function, and exported, because the alternative is a comparator inside
 * a component that can only be tested by mounting it.
 */
export const orderMemories = (memories: Memory[], now = today()): Memory[] =>
  [...memories].sort((a, b) => {
    const stale = Number(isStale(b, now)) - Number(isStale(a, now));
    if (stale) return stale;
    const pinned = Number(b.pinned) - Number(a.pinned);
    if (pinned) return pinned;
    return b.last_confirmed.localeCompare(a.last_confirmed);
  });

const DAY_MS = 24 * 60 * 60 * 1000;
const dayOf = (date: string) => Date.parse(`${date.slice(0, 10)}T00:00:00Z`);

/**
 * What confirming a memory writes: today, and the next review date moved on by
 * the same interval the memory was written with.
 *
 * Confirming used to move `last_confirmed` and nothing else. Now that the prompt
 * hedges a memory past its `review_after` and gives it up first, that left a
 * memory the person had just confirmed still marked out of date. The interval
 * is read off the memory itself — its review date minus the day it was last
 * confirmed — rather than from a table of kinds, so a date the model chose
 * ("until the trip in October") keeps its own rhythm and there is no second
 * copy of `memory::review_interval_days` to drift. A memory with no review
 * date keeps none; one whose dates cannot give an interval loses it rather
 * than stay stale after being confirmed.
 */
export const confirmedPatch = (memory: Memory, now = today()): Record<string, unknown> => {
  const patch: Record<string, unknown> = { last_confirmed: now };
  if (!memory.review_after) return patch;
  const gap = Math.round((dayOf(memory.review_after) - dayOf(memory.last_confirmed)) / DAY_MS);
  patch.review_after = Number.isFinite(gap) && gap > 0
    // Arithmetic on days, not a moment: `dayOf` reads a day as UTC midnight,
    // so UTC is the zone that gives the same day back.
    // eslint-disable-next-line no-restricted-syntax
    ? new Date(dayOf(now) + gap * DAY_MS).toISOString().slice(0, 10)
    : null;
  return patch;
};

/**
 * The vault path is needed only for the proposal tray, which is a file in
 * `Syn/` rather than a row in the index — memories themselves are read from the
 * database and written through `useNodeService`, both of which already know
 * which vault is open.
 */
export function useSynMemory(vaultPath: () => string) {
  const memories = ref<Memory[]>([]);
  const proposals = ref<Proposal[]>([]);
  const budget = ref<MemoryBudget | null>(null);
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  const ns = useNodeService();
  const asMessage = (e: unknown) => errorText(e);

  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      const [rows, spend, queued] = await Promise.all([
        invoke<Memory[]>('syn_list_memories'),
        invoke<MemoryBudget>('syn_memory_budget'),
        invoke<Proposal[]>('syn_list_proposals', { vaultPath: vaultPath() }),
      ]);
      memories.value = rows;
      budget.value = spend;
      proposals.value = queued;
    } catch (e) {
      logger.error('[Syn] Failed to read memories', e);
      error.value = asMessage(e);
      memories.value = [];
      budget.value = null;
      proposals.value = [];
    } finally {
      isLoading.value = false;
    }
  };

  /**
   * Change one thing about a memory.
   *
   * A property patch: keys not named are left exactly as they are on disk,
   * which is what lets this screen save without deleting a field somebody
   * added to the file by hand.
   */
  const patch = async (memory: Memory, properties: Record<string, unknown>) => {
    error.value = null;
    try {
      await ns.writeNode({ relPath: memory.id, nodeType: 'syn_memory', title: memory.title, properties });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to update a memory', e);
      error.value = asMessage(e);
    }
  };

  const setPinned = (memory: Memory, pinned: boolean) => patch(memory, { pinned });

  /** Confirming is what stops a memory going stale — it is a date, not a flag. */
  const confirm = (memory: Memory) => patch(memory, confirmedPatch(memory));

  /**
   * Forgetting is trashing, which is reversible — the vault's trash holds it
   * and `restore_node` brings it back. There is deliberately no hard delete
   * here: the whole guard on letting Syn write is that every act reverses.
   */
  const forget = async (memory: Memory) => {
    error.value = null;
    try {
      await ns.trashNode({ relPath: memory.id });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to forget a memory', e);
      error.value = asMessage(e);
    }
  };

  /**
   * Accept a suggestion: it becomes a real memory, written the same way one
   * Syn was told directly is written.
   */
  const accept = async (proposal: Proposal) => {
    error.value = null;
    try {
      await invoke('syn_accept_proposal', { vaultPath: vaultPath(), proposalId: proposal.id });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to accept a proposal', e);
      error.value = asMessage(e);
    }
  };

  /** Decline one. Nothing is written, and nothing is left behind. */
  const dismiss = async (proposal: Proposal) => {
    error.value = null;
    try {
      await invoke('syn_dismiss_proposal', { vaultPath: vaultPath(), proposalId: proposal.id });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to dismiss a proposal', e);
      error.value = asMessage(e);
    }
  };

  return {
    memories, proposals, budget, isLoading, error,
    load, setPinned, confirm, forget, accept, dismiss,
  };
}
