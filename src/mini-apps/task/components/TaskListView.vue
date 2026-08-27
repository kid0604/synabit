<script setup lang="ts">
import { computed } from 'vue';
import { CheckCircle2, Circle } from 'lucide-vue-next';
import DeleteButton from './DeleteButton.vue';
import { type TaskMetadata, isOverdue } from '../types';
import { buildTaskTree, flattenTaskTree, allSubtaskProgress, MAX_SUBTASK_DEPTH } from '../subtasks';
import TaskCardMeta from './TaskCardMeta.vue';

const props = defineProps<{
  /** How much a delete asks first; see `taskDeleteConfirm`. */
  deleteConfirm?: 'dialog' | 'inline' | 'undo';
  tasks: TaskMetadata[];
  /**
   * Every task, for counting subtask progress.
   *
   * Separate from `tasks`, which is what this view is showing. A parent in the
   * Today bucket has subtasks that are not, and counting only the visible ones
   * would report a number nobody asked about.
   */
  allTasks?: TaskMetadata[];
  /** Ids currently picked out. Empty means the checkboxes are not shown. */
  selectedIds?: Set<string>;
  /** The row the keyboard is on, if the keyboard is being used. */
  focusedId?: string | null;
  /**
   * Sections to draw, when something is being grouped by.
   *
   * Grouping and nesting are exclusive: a section headed "P1" cannot sensibly
   * hold a P3 subtask under its parent, and a subtask whose parent is in
   * another section would have to be drawn twice or not at all. So a grouped
   * list is flat, and the tree is what you get when nothing is grouped.
   */
  groups?: { key: string; label: string; literal: boolean; tasks: TaskMetadata[] }[];
}>();

const isSelecting = computed(() => (props.selectedIds?.size ?? 0) > 0);
const isSelected = (id: string) => props.selectedIds?.has(id) ?? false;

/**
 * The list, with subtasks sitting under their parents.
 *
 * Built from the tasks this view was given rather than from every task in the
 * vault, so a filtered view stays filtered: Today shows the subtasks due
 * today, and a subtask whose parent is not in the filter is shown at the top
 * level rather than hidden.
 */
/**
 * The sections to draw: one unnamed section holding the tree, or the groups
 * given, each one flat.
 */
const sections = computed(() => {
  const built = props.groups && props.groups.length > 1
    ? props.groups.map((g) => ({
        ...g,
        rows: g.tasks.map((task) => ({ task, depth: 0, children: [] as unknown[] })),
      }))
    : [{
        key: 'all', label: '', literal: true,
        rows: flattenTaskTree(buildTaskTree(props.groups?.[0]?.tasks ?? props.tasks)),
      }];

  // Each section's position in the whole list, so a row can tell whether it is
  // one of the first few — see `DEFER_AFTER`.
  let offset = 0;
  return built.map((section) => {
    const withOffset = { ...section, offset };
    offset += section.rows.length;
    return withOffset;
  });
});

/**
 * How many rows are drawn normally before the rest are deferred.
 *
 * `content-visibility: auto` lets the browser skip layout and paint for a row
 * that is not near the viewport. Applying it to rows that *are* on screen is
 * counter-productive: the browser has to evaluate the visibility boundary
 * before it can render them, which delays exactly the part the user is waiting
 * for. Twenty rows is comfortably more than fits on a tall screen.
 */
const DEFER_AFTER = 20;

/**
 * Indent stops after a few levels. Past that the title has no room left, and
 * a hand-edited file can nest as deep as it likes.
 */
const indentFor = (depth: number) => `${Math.min(depth, MAX_SUBTASK_DEPTH) * 24}px`;

// One pass for the whole list rather than one pass per parent row, which is
// what calling `subtaskProgress` from the template amounted to.
const progress = computed(() => allSubtaskProgress(props.allTasks ?? props.tasks));
const progressOf = (task: TaskMetadata) => progress.value.get(task.id) ?? { done: 0, total: 0 };

const emit = defineEmits<{
  (e: 'edit-task', task: TaskMetadata): void;
  (e: 'toggle-status', task: TaskMetadata): void;
  (e: 'delete-task', task: TaskMetadata): void;
  (e: 'open-person', transferredTo: string): void;
  (e: 'select-one', id: string): void;
  (e: 'select-range', id: string): void;
}>();

/**
 * A click on a row while a selection is running extends it rather than opening
 * the task. Opening one from inside a selection is almost never what was
 * meant, and losing the selection to a stray click is annoying enough that
 * people stop using the feature.
 */
const onRowClick = (event: MouseEvent, task: TaskMetadata) => {
  if (event.shiftKey) {
    emit('select-range', task.id);
    return;
  }
  if (isSelecting.value) {
    emit('select-one', task.id);
    return;
  }
  emit('edit-task', task);
};
</script>

<template>
  <div class="space-y-2 mt-4 max-w-4xl mx-auto">
    <template v-for="section in sections" :key="section.key">
      <h3
        v-if="section.label"
        class="text-[11px] font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-3 pt-4 pb-1 flex items-center gap-2"
      >
        {{ section.literal ? section.label : $t(section.label) }}
        <span class="text-gray-300 dark:text-gray-600 font-medium normal-case tracking-normal">{{ section.rows.length }}</span>
      </h3>

      <div v-for="(row, index) in section.rows" :key="row.task.id"
          :data-task-id="row.task.id"
          :style="{ marginLeft: indentFor(row.depth) }"
          class="group flex items-center p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-[#1a1a1a] border transition-colors cursor-pointer"
          :class="[
              section.offset + index >= DEFER_AFTER ? 'row-deferred' : '',
              row.task.status === 'done' ? 'opacity-50 border-transparent' :
              isOverdue(row.task) ? 'border-red-200 dark:border-red-900/50 bg-red-50/20 dark:bg-red-900/5' : 'border-transparent hover:border-gray-100 dark:hover:border-gray-800',
              // Where the keyboard is. A ring rather than a background, so it
              // reads on top of the overdue tint instead of replacing it.
              row.task.id === focusedId ? 'ring-2 ring-blue-400 dark:ring-blue-500 ring-offset-1 ring-offset-white dark:ring-offset-[#242424]' : ''
          ]"
          @click="onRowClick($event, row.task)"
      >
          <!--
            Visible on hover before anything is picked, and pinned open once
            something is. A checkbox that only appears after the first
            selection leaves no way to make the first selection.
          -->
          <label
              class="shrink-0 mr-3 hidden md:flex items-center cursor-pointer transition-opacity"
              :class="isSelecting ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus-within:opacity-100'"
              @click.stop
          >
              <input
                  type="checkbox"
                  :checked="isSelected(row.task.id)"
                  @click="$event.shiftKey ? emit('select-range', row.task.id) : emit('select-one', row.task.id)"
                  class="w-4 h-4 rounded border-gray-300 dark:border-gray-600 text-blue-500 focus:ring-blue-500 cursor-pointer"
                  :aria-label="$t('task.a11y_select_task')"
              />
          </label>

          <!-- Checkbox -->
          <button @click.stop="emit('toggle-status', row.task)" class="shrink-0 mr-4 transition-colors cursor-pointer" :aria-label="$t('task.a11y_toggle_status')">
              <CheckCircle2 v-if="row.task.status === 'done'" class="w-6 h-6 text-green-500 fill-green-50 dark:fill-green-900/30" />
              <Circle v-else class="w-6 h-6 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
          </button>

          <!-- Title & Meta -->
          <div class="flex-1 min-w-0 flex items-center justify-between">
              <p class="text-[15px] font-medium truncate transition-all duration-300" :class="row.task.status === 'done' ? 'text-gray-400 line-through' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'">
                  {{ row.task.title }}
              </p>
              <div class="hidden md:flex items-center gap-3 overflow-hidden ml-4 shrink-0">
                  <span v-if="row.task.status === 'in_progress'" class="text-[10px] px-2 py-0.5 rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 font-bold tracking-wider">DOING</span>

                  <TaskCardMeta :task="row.task" :progress="progressOf(row.task)" @open-person="emit('open-person', $event)" />
              </div>
          </div>

          <!-- Actions -->
          <div class="hidden md:flex shrink-0 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity items-center gap-1 ml-4 w-[60px] justify-end">
              <DeleteButton :mode="deleteConfirm" @confirm="emit('delete-task', row.task)" />
          </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
/**
 * Rows past the fold cost nothing to lay out until they are scrolled near.
 *
 * `contain-intrinsic-size` is not optional here: without it a deferred row
 * collapses to zero height while off screen, the scrollbar jumps around as the
 * list is scrolled, and the page fights the user. The `auto` keyword lets the
 * browser replace the estimate with the height the row actually turned out to
 * be, so it is right after the first time each row is seen.
 *
 * Nothing leaves the DOM, so find-in-page, the keyboard cursor, the
 * accessibility tree and printing all keep working — which is the whole reason
 * to reach for this before reaching for a virtual scroller.
 *
 * A WebView that does not know the property ignores it and renders every row,
 * which is what it did before.
 */
.row-deferred {
  content-visibility: auto;
  /* No intrinsic width — the row is a flex child and sizes itself. The 62px is
     one row at its usual height. */
  contain-intrinsic-size: auto none auto 62px;
}
</style>
