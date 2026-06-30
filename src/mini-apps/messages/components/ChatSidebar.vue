<script setup lang="ts">
import { ref } from 'vue';
import { Search, Hash } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import synAvatar from '../../../assets/syn-avatar.jpg';
import NavButtons from '../../../shared/components/NavButtons.vue';

export interface ChatContact {
  id: string;
  name: string;
  avatar?: string;
  lastMessagePreview?: string;
  timestamp?: string;
  unreadCount?: number;
  type: 'ai' | 'p2p' | 'group';
  isOnline?: boolean;
}

const props = defineProps<{
  contacts: ChatContact[];
  activeId: string | null;
}>();

const emit = defineEmits<{
  select: [id: string];
}>();

const { t } = useI18n();
const searchQuery = ref('');

const formatTime = (isoString?: string) => {
    if (!isoString) return '';
    const d = new Date(isoString);
    const now = new Date();
    if (d.toDateString() === now.toDateString()) {
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
};
</script>

<template>
  <div class="flex flex-col h-full bg-surface dark:bg-surface-dark border-r border-border dark:border-border-dark">
    <!-- Header -->
    <div class="h-14 flex items-center gap-3 px-4 flex-shrink-0 border-b border-border dark:border-border-dark" data-tauri-drag-region>
      <NavButtons />
      <h2 class="font-bold text-lg text-text dark:text-text-dark">Messages</h2>
    </div>

    <!-- Search -->
    <div class="px-3 py-3 border-b border-border dark:border-border-dark flex-shrink-0">
      <div class="relative">
        <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
          <Search class="w-4 h-4 text-gray-400" />
        </div>
        <input
          v-model="searchQuery"
          type="text"
          class="w-full bg-gray-100 dark:bg-[#1a1a1e] text-sm text-text dark:text-text-dark rounded-xl pl-9 pr-4 py-2 outline-none focus:ring-2 focus:ring-violet-500/50 transition-shadow placeholder-gray-400"
          placeholder="Search chats..."
        />
      </div>
    </div>

    <!-- Chat List -->
    <div class="flex-1 overflow-y-auto px-2 py-2 flex flex-col gap-1">
      <button
        v-for="contact in contacts"
        :key="contact.id"
        @click="emit('select', contact.id)"
        class="w-full flex items-center gap-3 p-2.5 rounded-xl text-left transition-colors cursor-pointer group relative"
        :class="activeId === contact.id ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
      >
        <!-- Avatar -->
        <div class="relative flex-shrink-0">
          <div class="w-12 h-12 rounded-xl overflow-hidden shadow-sm" :class="contact.type === 'ai' ? 'ring-1 ring-violet-500/30' : 'bg-gray-200 dark:bg-gray-800'">
             <img v-if="contact.avatar" :src="contact.avatar" alt="" class="w-full h-full object-cover" />
             <Hash v-else-if="contact.type === 'group'" class="w-6 h-6 m-3 text-gray-500" />
             <div v-else class="w-full h-full flex items-center justify-center text-lg font-bold text-gray-500">
               {{ contact.name.charAt(0).toUpperCase() }}
             </div>
          </div>
          <div v-if="contact.isOnline" class="absolute -bottom-0.5 -right-0.5 w-3.5 h-3.5 bg-green-500 rounded-full border-2 border-surface dark:border-surface-dark"></div>
        </div>

        <!-- Details -->
        <div class="flex-1 min-w-0 flex flex-col justify-center">
          <div class="flex items-center justify-between mb-0.5">
            <span class="font-semibold text-sm truncate" :class="activeId === contact.id ? 'text-violet-900 dark:text-violet-100' : 'text-gray-900 dark:text-gray-100'">
              {{ contact.name }}
            </span>
            <span class="text-[11px] flex-shrink-0" :class="contact.unreadCount ? 'text-violet-600 dark:text-violet-400 font-medium' : 'text-gray-400'">
              {{ formatTime(contact.timestamp) }}
            </span>
          </div>
          
          <div class="flex items-center justify-between gap-2">
             <p class="text-[13px] truncate" :class="contact.unreadCount ? 'text-gray-800 dark:text-gray-200 font-medium' : 'text-gray-500 dark:text-gray-400'">
               <span v-if="contact.type === 'ai' && !contact.lastMessagePreview" class="text-violet-600 dark:text-violet-400 opacity-80">AI Assistant</span>
               {{ contact.lastMessagePreview }}
             </p>
             <div v-if="contact.unreadCount" class="flex-shrink-0 min-w-[20px] h-[20px] rounded-full bg-red-500 text-white text-[11px] font-bold flex items-center justify-center px-1.5 shadow-sm">
               {{ contact.unreadCount > 99 ? '99+' : contact.unreadCount }}
             </div>
          </div>
        </div>
      </button>
    </div>
  </div>
</template>
