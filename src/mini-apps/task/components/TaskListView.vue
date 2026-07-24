<script setup lang="ts">
import { CheckCircle2, Circle, Trash2 } from 'lucide-vue-next';
import { type TaskMetadata, isOverdue } from '../types';
import TaskCardMeta from './TaskCardMeta.vue';

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
  <div class="space-y-2 mt-4 max-w-4xl mx-auto">
      <div v-for="task in tasks" :key="task.id" 
          class="group flex items-center p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-[#1a1a1a] border transition-colors cursor-pointer"
          :class="[
              task.status === 'done' ? 'opacity-50 border-transparent' : 
              isOverdue(task) ? 'border-red-200 dark:border-red-900/50 bg-red-50/20 dark:bg-red-900/5' : 'border-transparent hover:border-gray-100 dark:hover:border-gray-800'
          ]"
          @click="emit('edit-task', task)"
      >
          <!-- Checkbox -->
          <button @click.stop="emit('toggle-status', task)" class="shrink-0 mr-4 transition-colors cursor-pointer" aria-label="More Options">
              <CheckCircle2 v-if="task.status === 'done'" class="w-6 h-6 text-green-500 fill-green-50 dark:fill-green-900/30" />
              <Circle v-else class="w-6 h-6 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
          </button>
          
          <!-- Title & Meta -->
          <div class="flex-1 min-w-0 flex items-center justify-between">
              <p class="text-[15px] font-medium truncate transition-all duration-300" :class="task.status === 'done' ? 'text-gray-400 line-through' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'">
                  {{ task.title }}
              </p>
              <div class="hidden md:flex items-center gap-3 overflow-hidden ml-4 shrink-0">
                  <span v-if="task.status === 'in_progress'" class="text-[10px] px-2 py-0.5 rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 font-bold tracking-wider">DOING</span>
                  
                  <TaskCardMeta :task="task" @open-person="emit('open-person', $event)" />
              </div>
          </div>
          
          <!-- Actions -->
          <div class="hidden md:flex shrink-0 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity items-center gap-1 ml-4 w-[60px] justify-end">
              <button @click.stop="emit('delete-task', task)" class="p-1.5 text-gray-400 hover:text-red-500 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer" aria-label="More Options">
                  <Trash2 class="w-4 h-4" />
              </button>
          </div>
      </div>
  </div>
</template>
