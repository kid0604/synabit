<script setup lang="ts">
import { CalendarDays, Tag, User, Eye } from 'lucide-vue-next';
import { type TaskMetadata, getPriorityClass, getTransferredName, isLinkedPerson, isOverdue } from '../types';

defineProps<{
  task: TaskMetadata;
  compact?: boolean;  // true for board/matrix cards (smaller text)
}>();

const emit = defineEmits<{
  (e: 'open-person', transferredTo: string): void;
}>();
</script>

<template>
  <!-- Priority Badge -->
  <span v-if="task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold tracking-wider shrink-0"
    :class="[getPriorityClass(task.priority), compact ? 'px-1 py-[0.5px]' : 'px-2']">{{ task.priority }}</span>

  <!-- Transferred User -->
  <div v-if="task.is_transferred && task.transferred_to" 
    @click.stop="isLinkedPerson(task.transferred_to) ? emit('open-person', task.transferred_to) : null" 
    class="flex items-center shrink-0 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" 
    :class="isLinkedPerson(task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'">
    <User v-if="isLinkedPerson(task.transferred_to)" class="w-3 h-3 mr-1" />
    <span class="text-[10px] font-semibold truncate" :class="compact ? 'max-w-[100px]' : 'max-w-[120px]'">{{ getTransferredName(task.transferred_to) }}</span>
    <Eye v-if="task.track_progress" class="w-3 h-3 ml-1 text-blue-500" />
  </div>

  <!-- Dates -->
  <template v-if="compact">
    <span v-if="task.start_date || task.due_date" class="text-[10px] px-1.5 py-0.5 rounded flex items-center"
      :class="isOverdue(task) ? 'bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400 font-bold' : 'text-gray-500 bg-gray-100 dark:bg-[#2a2a2a]'">
      <CalendarDays class="w-3 h-3 mr-1" /> {{ task.start_date ? task.start_date.substring(5) : '--' }} - {{ task.due_date ? task.due_date.substring(5) : '--' }}
    </span>
  </template>
  <template v-else>
    <span v-if="task.due_date" class="text-xs flex items-center font-medium"
      :class="isOverdue(task) ? 'text-red-600 dark:text-red-400 bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded' : 'text-red-500'">
      <CalendarDays class="w-3 h-3 mr-1" /> {{ task.due_date }}
    </span>
    <span v-if="task.start_date" class="text-xs flex items-center text-blue-500 font-medium">
      <CalendarDays class="w-3 h-3 mr-1" /> {{ task.start_date }}
    </span>
  </template>

  <!-- Tags -->
  <template v-if="compact">
    <div v-if="task.tags.length" class="flex flex-wrap gap-1">
      <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">{{ tag }}</span>
    </div>
  </template>
  <template v-else>
    <span v-if="task.tags.length > 0" class="text-xs flex items-center text-gray-500 max-w-[150px] truncate">
      <Tag class="w-3 h-3 mr-1 shrink-0" /> {{ task.tags.join(', ') }}
    </span>
  </template>
</template>
