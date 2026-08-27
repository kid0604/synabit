import { ref, onUnmounted } from 'vue';
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
const UNDO_WINDOW_MS = 7000;

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
 * There is no confirmation dialog, deliberately. A dialog asks people to be
 * careful before the fact, which trains them to click through it; an undo lets
 * them be careless and still be fine. Only one of those two actually saves a
 * note.
 *
 * If the app quits inside the window, the deletion simply never happened —
 * the safe direction to fail in.
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
  saveTimeouts: Map<string, ReturnType<typeof setTimeout>>;
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
    tabAccessTime, saveTimeouts, ns, scanVault, onFailed,
  } = params;

  /** The notes waiting to go, with enough about each to put it back. */
  const pending = ref<{
    /** Ascending by index, which is the order they have to be reinserted in. */
    notes: { note: NoteItem; index: number }[];
    /** Whether the note being edited was among them. */
    wasCurrent: boolean;
  } | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Ids the list must pretend are gone.
   *
   * A pending note is still on disk, so any rescan — and the file watcher
   * fires plenty of them — would find it and put it straight back in the
   * sidebar underneath the toast offering to undo its deletion.
   */
  const hiddenIds = new Set<string>();

  const isHidden = (id: string) => hiddenIds.has(id);

  /** Move the files at last. Called by the timer, or early to make way. */
  const commit = async () => {
    const held = pending.value;
    if (!held) return;
    clearTimeout(timer);
    pending.value = null;
    for (const { note } of held.notes) hiddenIds.delete(note.id);

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
    }

    // One rescan for the whole batch. It is what puts any failure back in the
    // list, so it has to happen before anyone is told about one.
    await scanVault();
    for (const note of failed) onFailed(note);
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

    // One batch at a time. A second delete finishes the first rather than
    // queueing, so the toast never lies about what it is offering to bring
    // back.
    if (pending.value) await commit();

    const doomed = new Set(held.map((h) => h.note.id));
    const wasCurrent = currentNoteId.value !== null && doomed.has(currentNoteId.value);

    for (const id of doomed) {
      // Cancel the autosave before anything else: a 600ms timer firing after
      // this would write the note back to the path it is being taken off.
      const queued = saveTimeouts.get(id);
      if (queued) {
        clearTimeout(queued);
        saveTimeouts.delete(id);
      }

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

    pending.value = { notes: held, wasCurrent };
    timer = setTimeout(() => { void commit(); }, UNDO_WINDOW_MS);
  };

  const deleteNote = (id: string) => deleteNotes([id]);

  const undoDelete = () => {
    const held = pending.value;
    if (!held) return;
    clearTimeout(timer);
    pending.value = null;
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

  // Leaving the Notes app is not taking the delete back. The file has to go,
  // and it has to go before this composable stops existing to send it.
  onUnmounted(() => { void commit(); });

  return { pending, deleteNote, deleteNotes, undoDelete, commit, isHidden };
}
