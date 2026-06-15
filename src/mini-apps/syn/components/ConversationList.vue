<script setup lang="ts">
import { ref, computed } from 'vue';
import { Search, MessageCircle, Pin, PinOff, Trash2, Pencil, Sparkles, MoreVertical } from 'lucide-vue-next';
import type { SynConversation } from '../types';

const props = defineProps<{
  conversations: SynConversation[];
  activeId: string | null;
}>();

const emit = defineEmits<{
  select: [id: string];
  create: [];
  delete: [id: string];
  rename: [id: string, title: string];
  pin: [id: string, pinned: boolean];
}>();

const searchQuery = ref('');
const contextMenuId = ref<string | null>(null);
const renamingId = ref<string | null>(null);
const renameValue = ref('');

// Filter and group conversations
const filteredConversations = computed(() => {
  let list = [...props.conversations];
  if (searchQuery.value) {
    const q = searchQuery.value.toLowerCase();
    list = list.filter(c => c.title.toLowerCase().includes(q));
  }
  // Sort: pinned first, then by updated_at desc
  list.sort((a, b) => {
    if (a.pinned && !b.pinned) return -1;
    if (!a.pinned && b.pinned) return 1;
    return new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime();
  });
  return list;
});

const groupedConversations = computed(() => {
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const yesterday = new Date(today.getTime() - 86400000);
  const lastWeek = new Date(today.getTime() - 7 * 86400000);

  const groups: { label: string; key: string; items: SynConversation[] }[] = [
    { label: 'syn.time_today', key: 'today', items: [] },
    { label: 'syn.time_yesterday', key: 'yesterday', items: [] },
    { label: 'syn.time_last_7_days', key: 'week', items: [] },
    { label: 'syn.time_older', key: 'older', items: [] },
  ];

  for (const conv of filteredConversations.value) {
    const d = new Date(conv.updated_at);
    if (d >= today) {
      groups[0].items.push(conv);
    } else if (d >= yesterday) {
      groups[1].items.push(conv);
    } else if (d >= lastWeek) {
      groups[2].items.push(conv);
    } else {
      groups[3].items.push(conv);
    }
  }

  return groups.filter(g => g.items.length > 0);
});

const formatTime = (iso: string) => {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

const toggleContextMenu = (id: string) => {
  if (contextMenuId.value === id) {
    contextMenuId.value = null;
  } else {
    contextMenuId.value = id;
  }
};

const closeContextMenu = () => {
  contextMenuId.value = null;
};

const startRename = (conv: SynConversation) => {
  renamingId.value = conv.id;
  renameValue.value = conv.title;
  closeContextMenu();
};

const finishRename = () => {
  if (renamingId.value && renameValue.value.trim()) {
    emit('rename', renamingId.value, renameValue.value.trim());
  }
  renamingId.value = null;
  renameValue.value = '';
};

const handleDelete = (id: string) => {
  emit('delete', id);
  closeContextMenu();
};

const handlePin = (id: string) => {
  const conv = props.conversations.find(c => c.id === id);
  if (conv) {
    emit('pin', id, !conv.pinned);
  }
  closeContextMenu();
};
</script>

<template>
  <div
    class="flex flex-col h-full bg-gray-50/50 dark:bg-[#111115] select-none"
    @click="closeContextMenu"
  >
    <!-- New Chat button -->
    <div class="p-3">
      <button
        @click="emit('create')"
        class="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl bg-gradient-to-r from-violet-500 to-purple-600 text-white font-medium text-sm shadow-lg shadow-violet-500/20 hover:shadow-violet-500/30 hover:from-violet-600 hover:to-purple-700 transition-all cursor-pointer group"
      >
        <Sparkles class="w-4 h-4 group-hover:animate-pulse" />
        <span>{{ $t('syn.new_chat') }}</span>
      </button>
    </div>

    <!-- Search -->
    <div class="px-3 pb-2">
      <div class="relative">
        <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-gray-400" />
        <input
          v-model="searchQuery"
          type="text"
          :placeholder="$t('syn.search_conversations')"
          class="w-full pl-9 pr-3 py-2 rounded-lg bg-white dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 text-sm text-text dark:text-text-dark placeholder-gray-400 dark:placeholder-gray-500 outline-none focus:border-violet-400 dark:focus:border-violet-500/50 transition-colors"
        />
      </div>
    </div>

    <!-- Conversation list -->
    <div class="flex-1 overflow-y-auto px-2 pb-3">
      <!-- Empty state -->
      <div
        v-if="groupedConversations.length === 0"
        class="flex flex-col items-center justify-center py-12 text-gray-400 dark:text-gray-500"
      >
        <MessageCircle class="w-8 h-8 mb-2 opacity-40" />
        <p class="text-xs">{{ $t('syn.no_conversations') }}</p>
      </div>

      <!-- Groups -->
      <div v-for="group in groupedConversations" :key="group.key" class="mb-3">
        <p class="px-3 py-1.5 text-[11px] font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider">
          {{ $t(group.label) }}
        </p>
        <div class="flex flex-col gap-0.5">
          <div
            v-for="conv in group.items"
            :key="conv.id"
            @click="emit('select', conv.id)"
            class="w-full text-left px-3 py-2.5 rounded-lg transition-all group/item cursor-pointer relative"
            :class="conv.id === activeId
              ? 'bg-violet-50 dark:bg-violet-500/10 text-violet-700 dark:text-violet-300'
              : 'hover:bg-gray-100 dark:hover:bg-white/5 text-text dark:text-text-dark'"
          >
            <!-- Rename mode -->
            <div v-if="renamingId === conv.id" @click.stop>
              <input
                v-model="renameValue"
                @keydown.enter="finishRename"
                @keydown.escape="renamingId = null"
                @blur="finishRename"
                class="w-full px-2 py-1 rounded-md bg-white dark:bg-gray-800 border border-violet-400 text-sm outline-none"
                autofocus
              />
            </div>
            <!-- Normal mode -->
            <div v-else class="pr-6">
              <div class="flex items-center gap-1.5">
                <Pin v-if="conv.pinned" class="w-3 h-3 text-violet-500 flex-shrink-0" />
                <span class="text-sm font-medium truncate flex-1">{{ conv.title }}</span>
              </div>
              <div class="flex items-center gap-2 mt-0.5">
                <span class="text-[11px] text-gray-400 dark:text-gray-500">
                  {{ formatTime(conv.updated_at) }}
                </span>
                <span class="text-[11px] text-gray-400 dark:text-gray-500">
                  · {{ conv.message_count }} {{ $t('syn.messages_count') }}
                </span>
              </div>
            </div>
            
            <!-- Context Menu Toggle -->
            <div class="absolute right-2 top-2.5 md:opacity-0 opacity-100 group-hover/item:opacity-100 transition-opacity z-10" :class="{'md:opacity-100': contextMenuId === conv.id}">
               <button @click.stop="toggleContextMenu(conv.id)" class="p-1 rounded bg-white dark:bg-[#1e1f25] shadow-sm hover:bg-gray-100 dark:hover:bg-white/5 border border-gray-200 dark:border-gray-700">
                  <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
               </button>
               <div v-if="contextMenuId === conv.id" class="absolute right-0 top-7 w-36 bg-white dark:bg-[#1e1f25] shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden" @click.stop>
                  <button @click.stop="startRename(conv)" class="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 transition-colors cursor-pointer">
                    <Pencil class="w-3.5 h-3.5 text-gray-400" /> {{ $t('syn.rename') }}
                  </button>
                  <button @click.stop="handlePin(conv.id)" class="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 transition-colors cursor-pointer">
                    <template v-if="conv.pinned">
                      <PinOff class="w-3.5 h-3.5 text-gray-400" /> {{ $t('syn.unpin') }}
                    </template>
                    <template v-else>
                      <Pin class="w-3.5 h-3.5 text-gray-400" /> {{ $t('syn.pin') }}
                    </template>
                  </button>
                  <button @click.stop="handleDelete(conv.id)" class="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors cursor-pointer">
                    <Trash2 class="w-3.5 h-3.5" /> {{ $t('syn.delete') }}
                  </button>
               </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
