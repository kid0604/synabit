<script setup lang="ts">
import { computed } from 'vue';
import { CheckCircle2, Circle, User, Eye } from 'lucide-vue-next';
import DeleteButton from './DeleteButton.vue';
import { type TaskMetadata, getPriorityClass, getTransferredName, isLinkedPerson } from '../types';
import { buildTaskTree, flattenTaskTree, allSubtaskProgress, MAX_SUBTASK_DEPTH } from '../subtasks';

const props = defineProps<{
  /** How much a delete asks first; see `taskDeleteConfirm`. */
  deleteConfirm?: 'dialog' | 'inline' | 'undo';
  tasks: TaskMetadata[];
  /** Every task, for counting subtask progress; see `TaskListView`. */
  allTasks?: TaskMetadata[];
  /** Ids currently picked out. Empty means the checkboxes are not shown. */
  selectedIds?: Set<string>;
  /** The row the keyboard is on, if the keyboard is being used. */
  focusedId?: string | null;
}>();

const emit = defineEmits<{
  (e: 'edit-task', task: TaskMetadata): void;
  (e: 'toggle-status', task: TaskMetadata): void;
  (e: 'delete-task', task: TaskMetadata): void;
  (e: 'open-person', transferredTo: string): void;
  (e: 'select-one', id: string): void;
  (e: 'select-range', id: string): void;
}>();

// The same arrangement the list makes, so a task sits under its parent
// whichever of the two the user happens to be looking at.
const rows = computed(() => flattenTaskTree(buildTaskTree(props.tasks)));
const progress = computed(() => allSubtaskProgress(props.allTasks ?? props.tasks));
const progressOf = (task: TaskMetadata) => progress.value.get(task.id) ?? { done: 0, total: 0 };
const indentFor = (depth: number) => `${Math.min(depth, MAX_SUBTASK_DEPTH) * 20}px`;

const isSelecting = computed(() => (props.selectedIds?.size ?? 0) > 0);
const isSelected = (id: string) => props.selectedIds?.has(id) ?? false;

/**
 * A click while a selection is running extends it rather than opening the row.
 * Opening a task from inside a selection is almost never what was meant, and
 * losing the selection to a stray click is annoying enough to stop people
 * using it at all.
 */
const onRowClick = (event: MouseEvent, task: TaskMetadata) => {
  if (event.shiftKey) return emit('select-range', task.id);
  if (isSelecting.value) return emit('select-one', task.id);
  emit('edit-task', task);
};
</script>

<template>
  <div class="mt-6 border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl overflow-hidden bg-white dark:bg-[#1e1e1e]">
     <table class="w-full text-left text-sm">
         <thead class="bg-gray-50 dark:bg-[#1a1a1a] text-gray-500 dark:text-gray-400 text-xs uppercase font-semibold">
             <tr>
                 <th class="px-6 py-3 w-8"></th>
                 <th class="px-6 py-3 w-8">Status</th>
                 <th class="px-6 py-3">{{ $t('task.title_col') }}</th>
                 <th class="px-6 py-3 w-32">Start Date</th>
                 <th class="px-6 py-3 w-32">{{ $t('task.due_date_col') }}</th>
                 <th class="px-6 py-3 w-48">Tags</th>
                 <th class="px-6 py-3 w-16"></th>
             </tr>
         </thead>
         <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#2c2c2c]">
             <tr v-for="row in rows" :key="row.task.id"
                 :data-task-id="row.task.id"
                 class="hover:bg-gray-50 dark:hover:bg-[#252525] group cursor-pointer"
                 :class="row.task.id === focusedId ? 'bg-blue-50/60 dark:bg-blue-950/30 outline outline-2 -outline-offset-2 outline-blue-400 dark:outline-blue-500' : ''"
                 @click="onRowClick($event, row.task)">
                 <td class="pl-6 pr-0 py-3" @click.stop>
                     <input
                         type="checkbox"
                         :checked="isSelected(row.task.id)"
                         @click="$event.shiftKey ? emit('select-range', row.task.id) : emit('select-one', row.task.id)"
                         class="w-4 h-4 rounded border-gray-300 dark:border-gray-600 text-blue-500 focus:ring-blue-500 cursor-pointer transition-opacity"
                         :class="isSelecting ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus:opacity-100'"
                         :aria-label="$t('task.a11y_select_task')"
                     />
                 </td>
                 <td class="px-6 py-3">
                     <button @click.stop="emit('toggle-status', row.task)" class="transition-colors cursor-pointer block mt-1" :aria-label="$t('row.task.a11y_toggle_status')">
                          <CheckCircle2 v-if="row.task.status === 'done'" class="w-5 h-5 text-green-500" />
                          <Circle v-else class="w-5 h-5 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                      </button>
                 </td>
                 <td class="px-6 py-3 font-medium text-[#1c1c1e] dark:text-[#f4f4f5]" :class="row.task.status === 'done' ? 'line-through text-gray-400' : ''">
                   <div class="flex items-center gap-2" :style="{ paddingLeft: indentFor(row.depth) }">
                     <span v-if="progressOf(row.task).total" class="text-[10px] px-2 py-0.5 rounded-full bg-sky-100 text-sky-700 dark:bg-sky-900/30 dark:text-sky-300 font-bold tracking-wider shrink-0">
                       {{ progressOf(row.task).done }}/{{ progressOf(row.task).total }}
                     </span>
                     <span v-if="row.task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(row.task.priority)">{{ row.task.priority }}</span>
                     <div v-if="row.task.is_transferred && row.task.transferred_to" @click.stop="isLinkedPerson(row.task.transferred_to) ? emit('open-person', row.task.transferred_to) : null" class="flex items-center shrink-0 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" :class="isLinkedPerson(row.task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'" :title="$t('row.task.transferred_to') + getTransferredName(row.task.transferred_to)">
                         <User v-if="isLinkedPerson(row.task.transferred_to)" class="w-3 h-3 mr-1" />
                         <span class="text-[10px] font-semibold truncate max-w-[120px]">{{ getTransferredName(row.task.transferred_to) }}</span>
                         <Eye v-if="row.task.track_progress" class="w-3.5 h-3.5 ml-1.5 text-blue-500" :title="$t('row.task.tracking_progress')" />
                     </div>
                     {{ row.task.title }}
                   </div>
                 </td>
                 <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                     {{ row.task.start_date || '--/--/----' }}
                 </td>
                 <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                     {{ row.task.due_date || '--/--/----' }}
                 </td>
                 <td class="px-6 py-3">
                     <div class="flex flex-wrap gap-1">
                         <span v-for="tag in row.task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                             {{ tag }}
                         </span>
                     </div>
                 </td>
                 <td class="px-6 py-3">
                     <DeleteButton :mode="deleteConfirm" compact class="opacity-0 group-hover:opacity-100 transition-opacity" @confirm="emit('delete-task', row.task)" />
                 </td>
             </tr>
         </tbody>
     </table>
  </div>
</template>
