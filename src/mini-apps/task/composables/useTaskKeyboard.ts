import { ref, nextTick, onMounted, onUnmounted, watch, type Ref, type ComputedRef } from 'vue';
import type { TaskMetadata } from '../types';

/**
 * Driving the task list from the keyboard.
 *
 * A cursor moves through the rows, and the keys act on whatever it is on. The
 * cursor is an id rather than an index: the list is rebuilt from the database
 * on every watcher tick, and an index would quietly come to rest on a
 * different task each time one arrived.
 *
 * # Not while somebody is typing
 *
 * Every one of these is a bare letter, so the first thing the handler does is
 * work out whether the keystroke belongs to a field. A shortcut that fires
 * while the search box has focus does not look like a shortcut; it looks like
 * the app eating your input.
 *
 * Modified keystrokes are left alone too — `Cmd+N` opens a window, `Ctrl+C`
 * copies — with the single exception of select-all, which is claimed only when
 * a selection is already running and there is therefore something to select.
 */

/** Whether the keystroke belongs to whatever the user is typing into. */
function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === 'INPUT'
    || tag === 'TEXTAREA'
    || tag === 'SELECT'
    || el.isContentEditable
    // A dialog is its own world: whatever it is, the list is not what the keys
    // are meant for while one is open.
    || !!el.closest?.('[role="dialog"]');
}

export interface TaskKeyboardActions {
  createTask: () => void;
  openTask: (task: TaskMetadata) => void;
  toggleStatus: (task: TaskMetadata) => void;
  deleteTask: (task: TaskMetadata) => void;
  selectOne: (id: string) => void;
  selectRange: (id: string) => void;
  clearSelection: () => void;
  selectAllVisible: () => void;
  focusSearch: () => void;
  setViewMode: (mode: 'list' | 'board' | 'table' | 'matrix') => void;
  showHelp: () => void;
}

export function useTaskKeyboard(
  rows: ComputedRef<TaskMetadata[]>,
  hasSelection: ComputedRef<boolean>,
  /** Suspends every shortcut — a modal is up, or the app is elsewhere. */
  suspended: Ref<boolean>,
  actions: TaskKeyboardActions,
) {
  /** The row the keys act on, or null when the cursor has not been placed. */
  const focusedId = ref<string | null>(null);

  // A cursor left on a task that is no longer listed acts on nothing and looks
  // like the keyboard has stopped working. Move it to where that task was.
  watch(rows, (list, previous) => {
    if (!focusedId.value) return;
    if (list.some(t => t.id === focusedId.value)) return;
    const wasAt = (previous ?? []).findIndex(t => t.id === focusedId.value);
    focusedId.value = wasAt === -1 ? null : (list[Math.min(wasAt, list.length - 1)]?.id ?? null);
  });

  const focusedTask = (): TaskMetadata | null =>
    rows.value.find(t => t.id === focusedId.value) ?? null;

  /**
   * Bring the cursor's row into view.
   *
   * Without this the cursor walks off the bottom of the screen and the keys go
   * on working on a task nobody can see. `block: 'nearest'` scrolls the least
   * that will do, so stepping through a list does not jerk it about.
   *
   * A deferred row — see `content-visibility` in `TaskListView` — still has a
   * box and a height, so this finds and reaches it exactly as it would any
   * other. Rows off the DOM entirely, which is what a virtual scroller would
   * do, would not have that.
   */
  const revealFocused = () => {
    const id = focusedId.value;
    if (!id) return;
    void nextTick(() => {
      // Scanned rather than looked up with a selector. A task's id is its path
      // — slashes, dots, spaces, whatever the user named the folder — and
      // building an attribute selector out of that needs `CSS.escape`, which
      // is one more thing to be missing or to get wrong. Reading the attribute
      // off each row compares the strings directly and cannot be tricked.
      for (const row of document.querySelectorAll('[data-task-id]')) {
        if (row.getAttribute('data-task-id') === id) {
          row.scrollIntoView({ block: 'nearest' });
          return;
        }
      }
    });
  };

  const move = (delta: number) => {
    const list = rows.value;
    if (!list.length) return;
    const at = list.findIndex(t => t.id === focusedId.value);
    // No cursor yet: down starts at the top, up starts at the bottom, which is
    // what the first press of either key should obviously do.
    const next = at === -1
      ? (delta > 0 ? 0 : list.length - 1)
      : Math.min(Math.max(at + delta, 0), list.length - 1);
    focusedId.value = list[next].id;
    revealFocused();
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (suspended.value || isTyping(event.target)) return;

    // Select-all is the one modified key worth claiming, and only when a
    // selection is running — otherwise Cmd+A should still select the page.
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'a' && hasSelection.value) {
      event.preventDefault();
      actions.selectAllVisible();
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;

    const task = focusedTask();

    switch (event.key) {
      case 'j': case 'ArrowDown': event.preventDefault(); move(1); return;
      case 'k': case 'ArrowUp': event.preventDefault(); move(-1); return;

      case 'Enter':
        if (task) { event.preventDefault(); actions.openTask(task); }
        return;

      case 'x':
        if (task) {
          event.preventDefault();
          if (event.shiftKey) actions.selectRange(task.id);
          else actions.selectOne(task.id);
        }
        return;

      case ' ':
        if (task) { event.preventDefault(); actions.toggleStatus(task); }
        return;

      // Both of the keys a delete could plausibly be, since which one it is
      // depends on the keyboard.
      case 'Backspace': case 'Delete':
        if (task) { event.preventDefault(); actions.deleteTask(task); }
        return;

      case 'Escape':
        if (hasSelection.value) { event.preventDefault(); actions.clearSelection(); }
        else focusedId.value = null;
        return;

      case 'n': case 'c':
        event.preventDefault();
        actions.createTask();
        return;

      case '/':
        event.preventDefault();
        actions.focusSearch();
        return;

      case '?':
        event.preventDefault();
        actions.showHelp();
        return;

      case '1': event.preventDefault(); actions.setViewMode('list'); return;
      case '2': event.preventDefault(); actions.setViewMode('board'); return;
      case '3': event.preventDefault(); actions.setViewMode('table'); return;
      case '4': event.preventDefault(); actions.setViewMode('matrix'); return;
    }
  };

  onMounted(() => window.addEventListener('keydown', onKeyDown));
  onUnmounted(() => window.removeEventListener('keydown', onKeyDown));

  return { focusedId, move };
}
