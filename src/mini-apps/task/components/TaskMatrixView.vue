<script setup lang="ts">
import { CalendarDays, Calendar, CheckCircle2, Send, Trash2, User } from 'lucide-vue-next';
import { type TaskMetadata, getTransferredName } from '../types';

defineProps<{
  tasksByQuadrant: Record<string, TaskMetadata[]>;
}>();

const emit = defineEmits<{
  (e: 'edit-task', task: TaskMetadata): void;
  (e: 'toggle-status', task: TaskMetadata): void;
  (e: 'delete-task', task: TaskMetadata): void;
  (e: 'drag-start', event: DragEvent, task: TaskMetadata): void;
  (e: 'matrix-drop', event: DragEvent, quadrantId: string): void;
  (e: 'open-person', transferredTo: string): void;
}>();

const QUADRANTS = [
  { id: 'do_first', label: 'task.matrix_do_first', emptyLabel: 'task.matrix_do_first_empty', number: '1', color: 'red' },
  { id: 'schedule', label: 'task.matrix_schedule', emptyLabel: 'task.matrix_schedule_empty', number: '2', color: 'blue' },
  { id: 'delegate', label: 'task.matrix_delegate', emptyLabel: 'task.matrix_delegate_empty', number: '3', color: 'amber' },
  { id: 'eliminate', label: 'task.matrix_eliminate', emptyLabel: 'task.matrix_eliminate_empty', number: '4', color: 'gray' },
] as const;

const getRotation = (idx: number, quadrantIdx: number): string => {
  const rotations = [
    [-0.8, 0.6, -0.3],   // do_first
    [0.7, -0.5, 0.4],    // schedule
    [-0.6, 0.8, -0.4],   // delegate
    [0.5, -0.7, 0.3],    // eliminate
  ];
  return `rotate(${rotations[quadrantIdx][idx % 3]}deg)`;
};

// Color class maps for dynamic quadrant rendering
const colorClasses: Record<string, {
  container: string;
  badge: string;
  headerText: string;
  count: string;
  card: string;
  cardText: string;
  checkbox: string;
  priority: string;
  date: string;
  delete: string;
  emptyBg: string;
  emptyIcon: string;
  emptyText: string;
}> = {
  red: {
    container: 'border-red-200/40 dark:border-red-900/20 from-red-50/30 dark:from-red-950/5',
    badge: 'bg-red-500',
    headerText: 'text-red-600 dark:text-red-400',
    count: 'bg-red-500/10 dark:bg-red-500/15 text-red-500',
    card: 'bg-red-50 dark:bg-red-950/20 border-red-200/50 dark:border-red-800/30 shadow-[0_1px_3px_rgba(239,68,68,0.06)] hover:shadow-[0_6px_20px_rgba(239,68,68,0.12)] dark:shadow-[0_1px_3px_rgba(239,68,68,0.03)] dark:hover:shadow-[0_6px_20px_rgba(239,68,68,0.08)]',
    cardText: 'text-red-900 dark:text-red-200',
    checkbox: 'border-red-400/60 dark:border-red-500/40 hover:bg-red-200 dark:hover:bg-red-800/50',
    priority: 'bg-red-200/60 dark:bg-red-800/40 text-red-700 dark:text-red-300',
    date: 'text-red-500/70 dark:text-red-400/60',
    delete: 'text-red-300/50 dark:text-red-600/30',
    emptyBg: 'bg-red-100/50 dark:bg-red-900/10',
    emptyIcon: 'text-red-300 dark:text-red-700',
    emptyText: 'text-red-300 dark:text-red-700',
  },
  blue: {
    container: 'border-blue-200/40 dark:border-blue-900/20 from-blue-50/30 dark:from-blue-950/5',
    badge: 'bg-blue-500',
    headerText: 'text-blue-600 dark:text-blue-400',
    count: 'bg-blue-500/10 dark:bg-blue-500/15 text-blue-500',
    card: 'bg-blue-50 dark:bg-blue-950/20 border-blue-200/50 dark:border-blue-800/30 shadow-[0_1px_3px_rgba(59,130,246,0.06)] hover:shadow-[0_6px_20px_rgba(59,130,246,0.12)] dark:shadow-[0_1px_3px_rgba(59,130,246,0.03)] dark:hover:shadow-[0_6px_20px_rgba(59,130,246,0.08)]',
    cardText: 'text-blue-900 dark:text-blue-200',
    checkbox: 'border-blue-400/60 dark:border-blue-500/40 hover:bg-blue-200 dark:hover:bg-blue-800/50',
    priority: 'bg-blue-200/60 dark:bg-blue-800/40 text-blue-700 dark:text-blue-300',
    date: 'text-blue-500/70 dark:text-blue-400/60',
    delete: 'text-blue-300/50 dark:text-blue-600/30',
    emptyBg: 'bg-blue-100/50 dark:bg-blue-900/10',
    emptyIcon: 'text-blue-300 dark:text-blue-700',
    emptyText: 'text-blue-300 dark:text-blue-700',
  },
  amber: {
    container: 'border-amber-200/40 dark:border-amber-900/20 from-amber-50/30 dark:from-amber-950/5',
    badge: 'bg-amber-500',
    headerText: 'text-amber-600 dark:text-amber-400',
    count: 'bg-amber-500/10 dark:bg-amber-500/15 text-amber-500',
    card: 'bg-amber-50 dark:bg-amber-950/20 border-amber-200/50 dark:border-amber-800/30 shadow-[0_1px_3px_rgba(245,158,11,0.06)] hover:shadow-[0_6px_20px_rgba(245,158,11,0.12)] dark:shadow-[0_1px_3px_rgba(245,158,11,0.03)] dark:hover:shadow-[0_6px_20px_rgba(245,158,11,0.08)]',
    cardText: 'text-amber-900 dark:text-amber-200',
    checkbox: 'border-amber-400/60 dark:border-amber-500/40 hover:bg-amber-200 dark:hover:bg-amber-800/50',
    priority: 'bg-amber-200/60 dark:bg-amber-800/40 text-amber-700 dark:text-amber-300',
    date: 'text-amber-500/70 dark:text-amber-400/60',
    delete: 'text-amber-300/50 dark:text-amber-600/30',
    emptyBg: 'bg-amber-100/50 dark:bg-amber-900/10',
    emptyIcon: 'text-amber-300 dark:text-amber-700',
    emptyText: 'text-amber-300 dark:text-amber-700',
  },
  gray: {
    container: 'border-gray-200/40 dark:border-gray-800/50 from-gray-50/30 dark:from-gray-900/10',
    badge: 'bg-gray-400 dark:bg-gray-600',
    headerText: 'text-gray-500 dark:text-gray-400',
    count: 'bg-gray-200/50 dark:bg-gray-700/40 text-gray-500 dark:text-gray-400',
    card: 'bg-gray-100/70 dark:bg-gray-800/20 border-gray-200/40 dark:border-gray-700/30 shadow-[0_1px_3px_rgba(0,0,0,0.03)] hover:shadow-[0_6px_20px_rgba(0,0,0,0.06)] dark:shadow-[0_1px_3px_rgba(0,0,0,0.05)] dark:hover:shadow-[0_6px_20px_rgba(0,0,0,0.12)]',
    cardText: 'text-gray-600 dark:text-gray-400',
    checkbox: 'border-gray-400/50 dark:border-gray-500/30 hover:bg-gray-200 dark:hover:bg-gray-700',
    priority: 'bg-gray-200/60 dark:bg-gray-700/40 text-gray-600 dark:text-gray-400',
    date: 'text-gray-400/70 dark:text-gray-500/60',
    delete: 'text-gray-300/50 dark:text-gray-600/30',
    emptyBg: 'bg-gray-100/50 dark:bg-gray-800/30',
    emptyIcon: 'text-gray-300 dark:text-gray-700',
    emptyText: 'text-gray-300 dark:text-gray-700',
  },
};

// Empty state icons per quadrant
const emptyIcons: Record<string, string> = {
  do_first: 'check',
  schedule: 'calendar',
  delegate: 'send',
  eliminate: 'trash',
};
</script>

<template>
  <div class="mt-6 flex-1 min-h-0 flex flex-col pb-8">
      <!-- Axis Labels -->
      <div class="flex items-center gap-3 mb-4">
          <div class="flex items-center gap-1.5">
              <div class="w-5 h-5 rounded-md bg-red-500/10 dark:bg-red-500/20 flex items-center justify-center">
                  <span class="text-[8px] font-black text-red-500">!</span>
              </div>
              <span class="text-[10px] font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('task.matrix_urgent') }}</span>
          </div>
          <div class="flex-1 h-px bg-gradient-to-r from-red-200 via-gray-200 to-blue-200 dark:from-red-900/30 dark:via-gray-700 dark:to-blue-900/30"></div>
          <div class="flex items-center gap-1.5">
              <span class="text-[10px] font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('task.matrix_not_urgent') }}</span>
              <div class="w-5 h-5 rounded-md bg-blue-500/10 dark:bg-blue-500/20 flex items-center justify-center">
                  <Calendar class="w-3 h-3 text-blue-500"/>
              </div>
          </div>
      </div>
      
      <!-- 2×2 Grid -->
      <div class="grid grid-cols-1 md:grid-cols-2 gap-3 flex-1 min-h-0">
          <div v-for="(quadrant, qIdx) in QUADRANTS" :key="quadrant.id"
               class="flex flex-col rounded-2xl border bg-gradient-to-br to-transparent dark:to-transparent p-3 min-h-[180px] overflow-hidden"
               :class="colorClasses[quadrant.color].container"
               @dragover.prevent @drop="emit('matrix-drop', $event, quadrant.id)">
              <div class="flex items-center justify-between mb-2 shrink-0">
                  <div class="flex items-center gap-2">
                      <div class="w-5 h-5 rounded-md flex items-center justify-center" :class="colorClasses[quadrant.color].badge">
                          <span class="text-[9px] font-black text-white">{{ quadrant.number }}</span>
                      </div>
                      <h3 class="text-[11px] font-bold uppercase tracking-wider" :class="colorClasses[quadrant.color].headerText">{{ $t(quadrant.label) }}</h3>
                  </div>
                  <span class="text-[10px] min-w-[20px] text-center py-0.5 rounded-md font-bold" :class="colorClasses[quadrant.color].count">{{ tasksByQuadrant[quadrant.id].length }}</span>
              </div>
              <div class="flex-1 overflow-y-auto custom-scrollbar">
                  <div class="flex flex-wrap gap-2 content-start">
                      <div v-for="(task, idx) in tasksByQuadrant[quadrant.id]" :key="task.id"
                           draggable="true" @dragstart="emit('drag-start', $event, task)" @click="emit('edit-task', task)"
                           class="relative flex flex-col justify-between w-[calc(50%-4px)] min-w-[120px] max-w-[180px] h-[100px] p-2.5 rounded-xl cursor-grab active:cursor-grabbing group transition-all duration-200 hover:-translate-y-0.5 active:scale-[0.97] border"
                           :class="colorClasses[quadrant.color].card"
                           :style="{ transform: getRotation(idx, qIdx) }">
                          <button @click.stop="emit('toggle-status', task)" class="absolute top-1.5 right-1.5 shrink-0 cursor-pointer opacity-40 hover:opacity-100 transition-opacity z-10">
                              <div class="w-3.5 h-3.5 rounded-full border-[1.5px] transition-colors" :class="colorClasses[quadrant.color].checkbox"></div>
                          </button>
                          <p class="text-[12px] font-semibold leading-[1.35] line-clamp-3 pr-4" :class="colorClasses[quadrant.color].cardText">{{ task.title }}</p>
                          <div class="flex items-center gap-1 mt-auto pt-1">
                              <span v-if="task.priority" class="text-[8px] font-bold px-1 py-[0.5px] rounded" :class="colorClasses[quadrant.color].priority">{{ task.priority }}</span>
                              <span v-if="task.due_date" class="text-[8px] flex items-center gap-0.5" :class="colorClasses[quadrant.color].date">
                                  <CalendarDays class="w-2 h-2"/>{{ task.due_date.substring(5) }}
                              </span>
                              <!-- Delegate quadrant: show transferred user -->
                              <span v-if="quadrant.id === 'delegate' && task.is_transferred && task.transferred_to" class="text-[8px] text-purple-500 dark:text-purple-400 flex items-center gap-0.5 font-semibold">
                                  <User class="w-2 h-2"/>{{ getTransferredName(task.transferred_to).substring(0, 6) }}
                              </span>
                          </div>
                          <button @click.stop="emit('delete-task', task)" class="absolute bottom-1.5 right-1.5 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-all cursor-pointer p-0.5" :class="colorClasses[quadrant.color].delete">
                              <Trash2 class="w-2.5 h-2.5"/>
                          </button>
                      </div>
                  </div>
                  <div v-if="tasksByQuadrant[quadrant.id].length === 0" class="flex flex-col items-center justify-center h-full py-8">
                      <div class="w-10 h-10 rounded-xl flex items-center justify-center mb-2" :class="colorClasses[quadrant.color].emptyBg">
                          <CheckCircle2 v-if="emptyIcons[quadrant.id] === 'check'" class="w-5 h-5" :class="colorClasses[quadrant.color].emptyIcon"/>
                          <Calendar v-else-if="emptyIcons[quadrant.id] === 'calendar'" class="w-5 h-5" :class="colorClasses[quadrant.color].emptyIcon"/>
                          <Send v-else-if="emptyIcons[quadrant.id] === 'send'" class="w-5 h-5" :class="colorClasses[quadrant.color].emptyIcon"/>
                          <Trash2 v-else class="w-5 h-5" :class="colorClasses[quadrant.color].emptyIcon"/>
                      </div>
                      <p class="text-[11px] font-medium" :class="colorClasses[quadrant.color].emptyText">{{ $t(quadrant.emptyLabel) }}</p>
                  </div>
              </div>
          </div>
      </div>
      
      <!-- Bottom Axis Label -->
      <div class="flex items-center justify-center mt-3 gap-4">
          <div class="flex items-center gap-1.5 text-[10px] font-semibold text-gray-400 dark:text-gray-500">
              <div class="w-3 h-3 rounded bg-gradient-to-br from-red-400 to-blue-400 opacity-40"></div>
              ↑ {{ $t('task.matrix_important') }}
          </div>
          <span class="text-gray-300 dark:text-gray-700">·</span>
          <div class="flex items-center gap-1.5 text-[10px] font-semibold text-gray-400 dark:text-gray-500">
              <div class="w-3 h-3 rounded bg-gradient-to-br from-amber-400 to-gray-400 opacity-40"></div>
              ↓ {{ $t('task.matrix_not_important') }}
          </div>
      </div>
  </div>
</template>
