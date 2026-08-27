import { ref, computed, watch, type Ref, type ComputedRef } from 'vue';
import type { TaskMetadata } from '../types';
import { taskProperties, getTodayStr } from '../types';
import { repeats } from '../recurrence';
import { logger } from '../../../utils/logger';

/**
 * Picking out several tasks and doing one thing to all of them.
 *
 * The selection is a set of ids rather than a set of tasks: the list is
 * rebuilt from disk on every watcher tick, so holding the objects would mean
 * holding stale copies, and holding indices would mean the selection silently
 * shifting onto different tasks.
 *
 * It is also pruned against what is actually on screen. A task that is deleted,
 * filtered out, or moved into another bucket while selected would otherwise
 * stay in the set invisibly, and the next bulk action would reach a task the
 * user can no longer see.
 */
export function useTaskSelection(
  visibleTasks: ComputedRef<TaskMetadata[]>,
  tasks: Ref<TaskMetadata[]>,
  ns: any,
  showToast: (msg: string) => void,
  t: (key: string, named?: Record<string, unknown>) => string,
) {
  const selectedIds = ref<Set<string>>(new Set());

  /** Whether the checkboxes are shown at all. */
  const isSelecting = computed(() => selectedIds.value.size > 0);

  const isSelected = (id: string) => selectedIds.value.has(id);

  const toggle = (id: string) => {
    const next = new Set(selectedIds.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedIds.value = next;
  };

  const clear = () => {
    if (selectedIds.value.size) selectedIds.value = new Set();
  };

  /**
   * Extend the selection from the last click to this one.
   *
   * Shift-click over what is on screen, in the order it is drawn — which is
   * what "from here to there" means to someone looking at a list, and is not
   * the same as the order the tasks happen to be stored in.
   */
  const lastClickedId = ref<string | null>(null);

  const selectRange = (id: string) => {
    const ids = visibleTasks.value.map(task => task.id);
    const to = ids.indexOf(id);
    const from = lastClickedId.value ? ids.indexOf(lastClickedId.value) : -1;
    if (to === -1 || from === -1) {
      toggle(id);
      lastClickedId.value = id;
      return;
    }
    const next = new Set(selectedIds.value);
    for (let i = Math.min(from, to); i <= Math.max(from, to); i += 1) next.add(ids[i]);
    selectedIds.value = next;
    lastClickedId.value = id;
  };

  const selectOne = (id: string) => {
    toggle(id);
    lastClickedId.value = id;
  };

  const selectedTasks = computed(() =>
    visibleTasks.value.filter(task => selectedIds.value.has(task.id)),
  );

  const allVisibleSelected = computed(
    () => visibleTasks.value.length > 0 && visibleTasks.value.every(task => selectedIds.value.has(task.id)),
  );

  const toggleAllVisible = () => {
    if (allVisibleSelected.value) clear();
    else selectedIds.value = new Set(visibleTasks.value.map(task => task.id));
  };

  // Drop anything no longer on screen. Without this a bulk action could reach
  // a task the user cannot see and did not mean to include.
  watch(visibleTasks, (visible) => {
    if (!selectedIds.value.size) return;
    const onScreen = new Set(visible.map(task => task.id));
    const kept = new Set([...selectedIds.value].filter(id => onScreen.has(id)));
    if (kept.size !== selectedIds.value.size) selectedIds.value = kept;
  });

  /**
   * Apply one change to every selected task.
   *
   * Each task is written on its own and a failure stops the run, because
   * carrying on would leave the user with no idea how far it got. What did
   * succeed stays — these are ordinary edits, each undoable by hand, and
   * rolling back writes that already reached disk would be a second batch of
   * writes with its own way to fail half way.
   */
  const applyToSelection = async (
    change: (task: TaskMetadata) => Record<string, unknown> | null,
    successKey: string,
  ) => {
    const chosen = selectedTasks.value;
    if (!chosen.length) return;

    let changed = 0;
    try {
      for (const task of chosen) {
        const overrides = change(task);
        if (!overrides) continue;
        await ns.writeNode({
          relPath: task.path,
          nodeType: 'task',
          title: task.title,
          properties: taskProperties(task, overrides),
        });
        Object.assign(task, overrides);
        changed += 1;
      }
      showToast(t(successKey, { count: changed }));
      clear();
    } catch (e) {
      logger.error('Bulk edit failed part way through', e);
      showToast(t('task.bulk_failed', { done: changed, total: chosen.length }));
    }
  };

  /**
   * Mark every selected task done.
   *
   * A repeating task among them is left alone rather than quietly advanced:
   * completing one is a decision about a single occurrence, and doing it to
   * twenty tasks at once is not what anyone means by "mark these done".
   */
  const completeSelected = async () => {
    const skipped = selectedTasks.value.filter(repeats).length;
    await applyToSelection(
      (task) => (repeats(task) || task.status === 'done'
        ? null
        : { status: 'done', completed_at: getTodayStr() }),
      'task.bulk_completed',
    );
    if (skipped > 0) showToast(t('task.bulk_skipped_repeating', { count: skipped }));
  };

  const setPriorityOnSelection = (priority: string) =>
    applyToSelection(
      (task) => (task.priority === priority ? null : { priority }),
      'task.bulk_priority_set',
    );

  const setProjectOnSelection = (projectId: string) =>
    applyToSelection(
      (task) => (task.project_id === projectId ? null : { project_id: projectId }),
      'task.bulk_project_set',
    );

  return {
    selectedIds, selectedTasks, isSelecting, isSelected,
    selectOne, selectRange, clear, toggleAllVisible, allVisibleSelected,
    completeSelected, setPriorityOnSelection, setProjectOnSelection,
    tasks,
  };
}
