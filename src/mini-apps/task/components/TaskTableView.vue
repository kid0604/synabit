<script setup lang="ts">
import { CheckCircle2, Circle, Trash2, User, Eye } from 'lucide-vue-next';
import { type TaskMetadata, getPriorityClass, getTransferredName, isLinkedPerson } from '../types';

defineProps<{
  tasks: TaskMetadata[];
}>();

const emit = defineEmits<{
  (e: 'edit-task', task: TaskMetadata): void;
  (e: 'toggle-status', task: TaskMetadata): void;
  (e: 'delete-task', task: TaskMetadata): void;
  (e: 'open-person', transferredTo: string): void;
}>();
</script>

<template>
  <div class="mt-6 border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl overflow-hidden bg-white dark:bg-[#1e1e1e]">
     <table class="w-full text-left text-sm">
         <thead class="bg-gray-50 dark:bg-[#1a1a1a] text-gray-500 dark:text-gray-400 text-xs uppercase font-semibold">
             <tr>
                 <th class="px-6 py-3 w-8">Status</th>
                 <th class="px-6 py-3">{{ $t('task.title_col') }}</th>
                 <th class="px-6 py-3 w-32">Start Date</th>
                 <th class="px-6 py-3 w-32">{{ $t('task.due_date_col') }}</th>
                 <th class="px-6 py-3 w-48">Tags</th>
                 <th class="px-6 py-3 w-16"></th>
             </tr>
         </thead>
         <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#2c2c2c]">
             <tr v-for="task in tasks" :key="task.id" class="hover:bg-gray-50 dark:hover:bg-[#252525] group cursor-pointer" @click="emit('edit-task', task)">
                 <td class="px-6 py-3">
                     <button @click.stop="emit('toggle-status', task)" class="transition-colors cursor-pointer block mt-1">
                          <CheckCircle2 v-if="task.status === 'done'" class="w-5 h-5 text-green-500" />
                          <Circle v-else class="w-5 h-5 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                      </button>
                 </td>
                 <td class="px-6 py-3 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2" :class="task.status === 'done' ? 'line-through text-gray-400' : ''">
                     <span v-if="task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(task.priority)">{{ task.priority }}</span>
                     <div v-if="task.is_transferred && task.transferred_to" @click.stop="isLinkedPerson(task.transferred_to) ? emit('open-person', task.transferred_to) : null" class="flex items-center shrink-0 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" :class="isLinkedPerson(task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'" :title="$t('task.transferred_to') + getTransferredName(task.transferred_to)">
                         <User v-if="isLinkedPerson(task.transferred_to)" class="w-3 h-3 mr-1" />
                         <span class="text-[10px] font-semibold truncate max-w-[120px]">{{ getTransferredName(task.transferred_to) }}</span>
                         <Eye v-if="task.track_progress" class="w-3.5 h-3.5 ml-1.5 text-blue-500" :title="$t('task.tracking_progress')" />
                     </div>
                     {{ task.title }}
                 </td>
                 <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                     {{ task.start_date || '--/--/----' }}
                 </td>
                 <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                     {{ task.due_date || '--/--/----' }}
                 </td>
                 <td class="px-6 py-3">
                     <div class="flex flex-wrap gap-1">
                         <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                             {{ tag }}
                         </span>
                     </div>
                 </td>
                 <td class="px-6 py-3">
                     <button @click.stop="emit('delete-task', task)" class="p-1 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                         <Trash2 class="w-4 h-4" />
                     </button>
                 </td>
             </tr>
         </tbody>
     </table>
  </div>
</template>
