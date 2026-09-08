<script setup lang="ts">
import { computed } from 'vue';
import { CheckSquare, Calendar, Gift, MessageSquare, ArrowRight, Hourglass, Split, TriangleAlert } from 'lucide-vue-next';

const props = defineProps<{
  notification: any;
}>();

const emit = defineEmits(['action']);

const getIcon = (type: string) => {
  if (type === 'task_due') return CheckSquare;
  if (type === 'event_upcoming') return Calendar;
  if (type === 'birthday_upcoming') return Gift;
  // Things Syn noticed rather than things the calendar is announcing. The
  // subtypes come from `syn::notice::Kind::subtype`.
  if (type === 'syn_stuck_thread') return Hourglass;
  if (type === 'syn_contradiction') return Split;
  if (type === 'syn_degrading_skill') return TriangleAlert;
  return MessageSquare;
};

/**
 * Whether this card is Syn noticing something, rather than the calendar
 * reminding about something.
 *
 * They are two different acts and the card should not pretend otherwise. A
 * reminder is time-bound and the app owes it to you; a notice is a colleague
 * saying they spotted something and did nothing about it. The tint is the only
 * difference in weight, deliberately — a notice that shouted would be an
 * interruption, and interrupting has its own contract and is not this.
 */
const isNotice = computed(() => String(props.notification?.subtype ?? '').startsWith('syn_'));

const formatTime = (isoString?: string) => {
    if (!isoString) return '';
    const d = new Date(isoString);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) + ' · ' + d.toLocaleDateString();
};
</script>

<template>
  <div class="flex flex-col gap-1 min-w-0 flex-1 my-2" style="animation: messageIn 0.25s ease-out forwards;">
      <div class="flex items-baseline gap-2 ml-1">
          <span class="text-sm font-semibold text-gray-700 dark:text-gray-300">{{ notification.sender?.name || $t('chat.system_name') || 'Synabit System' }}</span>
          <span class="text-xs text-gray-400">{{ formatTime(notification.timestamp) }}</span>
      </div>
      
      <div class="bg-white dark:bg-surface-dark border border-gray-100 dark:border-border-dark shadow-sm rounded-2xl p-4 flex flex-col gap-3 relative overflow-hidden group w-full max-w-[80%] hover:border-violet-200 dark:hover:border-violet-500/30 transition-colors">
          
          <div class="flex items-start gap-3">
              <div
                class="p-2 rounded-lg flex-shrink-0 mt-0.5"
                :class="isNotice
                  ? 'bg-violet-50 dark:bg-violet-900/20 text-violet-500'
                  : 'bg-blue-50 dark:bg-blue-900/20 text-blue-500'"
              >
                  <component :is="getIcon(notification.subtype)" class="w-5 h-5" />
              </div>
              <div class="flex-1">
                  <h4 class="font-bold text-gray-900 dark:text-white leading-tight mb-1">{{ notification.content?.title }}</h4>
                  <p class="text-sm text-gray-600 dark:text-gray-400 whitespace-pre-wrap">{{ notification.content?.text }}</p>
              </div>
          </div>
          
          <div v-if="notification.content?.metadata?.target_id" class="border-t border-gray-100 dark:border-gray-800/60 pt-3 mt-1 flex justify-end">
              <button 
                  @click="emit('action', notification)"
                  class="text-sm font-medium text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300 flex items-center gap-1.5 transition-colors cursor-pointer"
              >
                  {{ $t('chat.view_details') || 'View Details' }}
                  <ArrowRight class="w-4 h-4" />
              </button>
          </div>
      </div>
  </div>
</template>

<style scoped>
@keyframes messageIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}
</style>
