import { ref, onBeforeUnmount, onDeactivated, getCurrentInstance } from 'vue';
import type { Ref } from 'vue';
import type { NoteItem } from '../helpers';
import { rememberRecentNotes } from '../helpers';
import { logger } from '../../../utils/logger';

/**
 * How long a deleted note stays undoable before it is actually moved.
 *
 * Long enough to notice the mistake and reach the button, short enough that
 * the note is not still hanging around when the user has moved on.
 */
export const UNDO_WINDOW_MS = 10000;

/**
 * Deleting a note, with the delete held back long enough to take it back.
 *
 * Nothing on disk is touched until the window closes. That is not a detail —
 * it is the whole reason there is no `restore` on the Rust side. Sync spots a
 * deletion by noticing a tracked path no longer holds a file, so the instant
 * the file moves, a tombstone is on its way to every other device; undoing
 * after that would be a race against the tombstone, and the tombstone would
 * sometimes win. Holding a timer is not a race at all.
 *
 * This composable asks nothing. The one yes/no a delete may have — the
 * app-wide "Ask before deleting" (`confirmDelete`), off by default — is asked
 * by the caller before a note is handed over; the undo is the way back.
 *
 * If the app is killed inside the window, the deletion simply never happened —
 * the safe direction to fail in. Leaving the app, or its window being hidden,
 * commits first, so that is rare.
 *
 * A delete carries a *set* of notes, not one. Tidying up after a sync that
 * left thirteen copies of the same day means deleting thirteen things, and
 * doing that one at a time gives thirteen toasts, each cancelling the last —
 * so the only note that stays undoable is the final one. One batch, one
 * window, one undo that puts every note back where it was.
 */
export function useNoteDelete(params: {
  notes: Ref<NoteItem[]>;
  currentNoteId: Ref<string | null>;
  recentNoteIds: Ref<string[]>;
  tabContents: Ref<Record<string, string>>;
  activeTabs: Ref<string[]>;
  tabAccessTime: Map<string, number>;
  /**
   * Write a tab now if an autosave is still waiting (`useNoteSave`). It reads
   * the tab synchronously, before its first `await`, so the words are taken
   * before the tab is closed below.
   */
  flushSave: (id: string) => Promise<void>;
  ns: { trashNode: (p: { relPath: string }) => Promise<string> };
  scanVault: () => Promise<void>;
  /**
   * Called when the file could not be moved after all.
   *
   * The note left the list the moment it was deleted, so a silent failure
   * looks exactly like a successful delete until the next restart brings it
   * back. Somebody has to say so, and it cannot be this file — a composable
   * that opens dialogs is a composable that cannot be tested.
   */
  onFailed: (note: NoteItem) => void;
}) {
  const {
    notes, currentNoteId, recentNoteIds, tabContents, activeTabs,
    tabAccessTime, flushSave, ns, scanVault, onFailed,
  } = params;

  /** The notes waiting to go, with enough about each to put it back. */
  const pending = ref<{
    /** Ascending by index, which is the order they have to be reinserted in. */
    notes: { note: NoteItem; index: number }[];
    /** Whether the note being edited was among them. */
    wasCurrent: boolean;
    /** Unsaved edits being written out; the trash must wait for them. */
    flushed: Promise<unknown>;
  } | null>(null);
  type Held = NonNullable<typeof pending.value>;
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** When the running timer fires, so a pause can keep what was left. */
  let deadline = 0;
  let remaining = UNDO_WINDOW_MS;

  /**
   * Ids the list must pretend are gone.
   *
   * A pending note is still on disk, so any rescan — and the file watcher
   * fires plenty of them — would find it and put it straight back in the
   * sidebar underneath the toast offering to undo its deletion. An id leaves
   * only once its file is in the trash (or the move failed): any earlier and
   * a rescan landing mid-commit flashes the note back.
   */
  const hiddenIds = new Set<string>();

  const isHidden = (id: string) => hiddenIds.has(id);

  /**
   * Take the waiting batch off the toast, synchronously — before any `await`,
   * or a delete started while the previous one is being written can be
   * overwritten and never happen (see `useUndoableAction`).
   */
  const take = (): Held | null => {
    clearTimeout(timer);
    timer = undefined;
    const held = pending.value;
    pending.value = null;
    return held;
  };

  const arm = (ms: number) => {
    clearTimeout(timer);
    deadline = Date.now() + ms;
    timer = setTimeout(() => { void commit(); }, ms);
  };

  const perform = async (held: Held) => {
    // A write still landing after the move would put the file back.
    await held.flushed;

    // Each note is moved on its own so one that cannot be moved does not
    // strand the rest — a batch that gives up halfway would leave the list
    // and the disk disagreeing about which notes still exist.
    const failed: NoteItem[] = [];
    for (const { note } of held.notes) {
      try {
        await ns.trashNode({ relPath: note.id });
      } catch (e) {
        logger.error('Could not move the note to the trash', e);
        failed.push(note);
      }
      hiddenIds.delete(note.id);
    }

    // One rescan for the whole batch. It is what puts any failure back in the
    // list, so it has to happen before anyone is told about one. A rescan that
    // fails is not a delete that failed: the moves above are what count.
    try {
      await scanVault();
    } catch (e) {
      logger.error('Rescan after deleting notes failed', e);
    }
    for (const note of failed) onFailed(note);
  };

  /** Move the files at last. Called by the timer, or early to make way. */
  const commit = async () => {
    const held = take();
    if (held) await perform(held);
  };

  /** Delete every note named, as one undoable step. */
  const deleteNotes = async (ids: string[]) => {
    // Indexes are read against the list as it stands now, so they are taken
    // before anything is removed and kept ascending — reinserting in that
    // order is what lands each note back on its own row rather than one along.
    const held = ids
      .map((id) => ({ index: notes.value.findIndex((n) => n.id === id), id }))
      .filter((e) => e.index !== -1)
      .sort((a, b) => a.index - b.index)
      .map((e) => ({ note: notes.value[e.index], index: e.index }));
    if (held.length === 0) return;

    const doomed = new Set(held.map((h) => h.note.id));
    const wasCurrent = currentNoteId.value !== null && doomed.has(currentNoteId.value);

    const flushes: Promise<unknown>[] = [];
    for (const id of doomed) {
      // Write out what is still waiting on the autosave before the tab is
      // closed. Cancelling it instead lost the last few seconds of typing —
      // an undo brought the note back without them, and the trash kept an
      // older copy. Started now, while the note and its tab still exist;
      // the move waits for it. Asked even with no autosave queued: the editor
      // may still be holding words it has not handed over, and a no-op
      // otherwise.
      flushes.push(flushSave(id).catch((e) => logger.error('Could not save a note being deleted', e)));

      hiddenIds.add(id);
      delete tabContents.value[id];
      tabAccessTime.delete(id);
    }

    notes.value = notes.value.filter((n) => !doomed.has(n.id));
    activeTabs.value = activeTabs.value.filter((t) => !doomed.has(t));
    if (wasCurrent) currentNoteId.value = notes.value[0]?.id ?? null;
    if (recentNoteIds.value.some((x) => doomed.has(x))) {
      recentNoteIds.value = recentNoteIds.value.filter((x) => !doomed.has(x));
      rememberRecentNotes(recentNoteIds.value);
    }

    // One batch at a time. A second delete finishes the first rather than
    // queueing, so the toast never lies about what it is offering to bring
    // back. The first is taken synchronously; its move is awaited only after
    // the new one holds the toast.
    const previous = take();
    pending.value = { notes: held, wasCurrent, flushed: Promise.all(flushes) };
    remaining = UNDO_WINDOW_MS;
    arm(UNDO_WINDOW_MS);
    if (previous) await perform(previous);
  };

  const deleteNote = (id: string) => deleteNotes([id]);

  const undoDelete = () => {
    const held = take();
    if (!held) return;
    for (const { note } of held.notes) hiddenIds.delete(note.id);

    // Back where they were, rather than on top. The list has an order the
    // reader recognises, and a note that jumps position on being restored
    // looks like a different note. Ascending order matters: each insert
    // shifts everything after it, so putting the earliest back first is what
    // makes the later indexes still mean what they meant.
    const restored = [...notes.value];
    for (const { note, index } of held.notes) {
      restored.splice(Math.min(index, restored.length), 0, note);
    }
    notes.value = restored;
    if (held.wasCurrent) currentNoteId.value = held.notes[0].note.id;
  };

  /** Hold the countdown while the pointer or focus is on the toast (WCAG 2.2.1). */
  const pause = () => {
    if (!pending.value || timer === undefined) return;
    remaining = Math.max(0, deadline - Date.now());
    clearTimeout(timer);
    timer = undefined;
  };

  const resume = () => {
    if (pending.value && timer === undefined) arm(remaining);
  };

  // Leaving the Notes app is not taking the delete back. The file has to go,
  // and it has to go while the undo can still be seen: the apps live in a
  // `<keep-alive>`, so switching app deactivates rather than unmounts, and
  // Android kills a hidden app without warning.
  const onHidden = () => {
    if (document.visibilityState === 'hidden') void commit();
  };
  if (getCurrentInstance()) {
    document.addEventListener('visibilitychange', onHidden);
    onDeactivated(() => { void commit(); });
    onBeforeUnmount(() => {
      document.removeEventListener('visibilitychange', onHidden);
      void commit();
    });
  }

  return { pending, deleteNote, deleteNotes, undoDelete, commit, pause, resume, isHidden };
}
