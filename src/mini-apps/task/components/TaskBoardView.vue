<script setup lang="ts">
import { Plus, Trash2 } from 'lucide-vue-next';
import { type TaskMetadata, isOverdue } from '../types';
import TaskCardMeta from './TaskCardMeta.vue';

defineProps<{
  tasksByStatus: Record<string, TaskMetadata[]>;
  columns: readonly { id: string; name: string; class: string }[];
  wipLimit: number;
  quickAddColumn: string | null;
  quickAddTitle: string;
}>();

const emit = defineEmits<{
  (e: 'edit-task', task: TaskMetadata): void;
  (e: 'delete-task', task: TaskMetadata): void;
  (e: 'drag-start', event: DragEvent, task: TaskMetadata): void;
  (e: 'drop', event: DragEvent, status: string): void;
  (e: 'show-quick-add', colId: string): void;
  (e: 'quick-add', status: string): void;
  (e: 'open-person', transferredTo: string): void;
  (e: 'update:quickAddColumn', value: string | null): void;
  (e: 'update:quickAddTitle', value: string): void;
}>();
</script>

<template>
  <div class="flex gap-6 flex-1 mt-6 pb-8 overflow-x-auto min-h-0 items-stretch">
      <div v-for="col in columns" :key="col.id" 
           class="flex-1 min-w-[280px] flex flex-col bg-gray-50/50 dark:bg-[#161616] rounded-2xl p-4 border border-[#e6e6e6] dark:border-[#2c2c2c]"
           @dragover.prevent 
           @drop="emit('drop', $event, col.id)"
      >
          <div class="flex items-center justify-between mb-4 px-1" :class="col.class">
              <h3 class="text-xs font-bold text-gray-500 pt-3 flex items-center">
                  {{ col.name }} 
                  <span class="ml-2 px-2 py-0.5 rounded-full transition-colors" 
                        :class="(col.id === 'in_progress' && tasksByStatus[col.id].length > wipLimit) ? 'bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400 font-bold' : 'bg-gray-200 dark:bg-[#2a2a2a] text-gray-600 dark:text-gray-300'">
                      {{ tasksByStatus[col.id].length }}
                  </span>
              </h3>
              <button @click="emit('show-quick-add', col.id)" class="text-gray-400 hover:text-black dark:hover:text-white pt-3" aria-label="More Options"><Plus class="w-4 h-4"/></button>
          </div>
          <div class="flex-1 overflow-y-auto space-y-3 pb-4 column-content">
              <div v-for="task in tasksByStatus[col.id]" :key="task.id"
                   draggable="true"
                   @dragstart="emit('drag-start', $event, task)"
                   @click="emit('edit-task', task)"
                   :data-task-id="task.id"
                   class="task-card p-4 rounded-xl border hover:shadow-md transition-shadow cursor-grab active:cursor-grabbing group relative"
                   :class="isOverdue(task) ? 'border-red-300 dark:border-red-900 bg-red-50/50 dark:bg-red-900/10' : 'bg-white dark:bg-[#1e1e1e] border-[#e6e6e6] dark:border-[#2c2c2c]'"
              >
                 <p class="text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] leading-snug mb-3">{{ task.title }}</p>
                 <div class="flex items-center justify-between mt-auto pt-2 border-t border-gray-50 dark:border-[#2c2c2c]">
                     <div class="flex gap-2 items-center flex-wrap">
                         <TaskCardMeta :task="task" compact @open-person="emit('open-person', $event)" />
                     </div>
                     <button @click.stop="emit('delete-task', task)" class="text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer" aria-label="More Options">
                         <Trash2 class="w-3.5 h-3.5" />
                     </button>
                 </div>
              </div>
              
              <!-- Quick Add Input -->
              <div v-if="quickAddColumn === col.id" class="mt-2 bg-white dark:bg-[#1e1e1e] p-3 rounded-xl border border-indigo-300 dark:border-indigo-500 shadow-sm animate-in fade-in zoom-in duration-200 shrink-0">
                  <input :id="'quick-add-input-' + col.id" 
                         type="text" 
                         :value="quickAddTitle"
                         @input="emit('update:quickAddTitle', ($event.target as HTMLInputElement).value)"
                         @keyup.enter="emit('quick-add', col.id)"
                         @keyup.esc="emit('update:quickAddColumn', null)"
                         @blur="!quickAddTitle.trim() ? emit('update:quickAddColumn', null) : null"
                         :placeholder="$t('task.task_title_placeholder')" 
                         class="w-full bg-transparent text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] outline-none placeholder:font-normal placeholder:text-gray-400"
                  />
              </div>
          </div>
      </div>
  </div>
</template>
