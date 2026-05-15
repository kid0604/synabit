<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Bot, Calendar, CheckSquare, Gift, ArrowRight, MessageSquare } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import NavButtons from '../../shared/components/NavButtons.vue';

const props = defineProps<{
    vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

const messages = ref<any[]>([]);
const loading = ref(true);
const chatContainer = ref<HTMLElement | null>(null);

const fetchMessages = async () => {
    loading.value = true;
    try {
        const history = await invoke<any[]>('get_chat_history', { vaultPath: props.vaultPath });
        messages.value = history;
        
        await nextTick();
        scrollToBottom();
    } catch (e) {
        logger.error('Failed to fetch messages', e);
    } finally {
        loading.value = false;
    }
};

onMounted(() => {
    fetchMessages();
});

const scrollToBottom = () => {
    if (chatContainer.value) {
        chatContainer.value.scrollTop = chatContainer.value.scrollHeight;
    }
};

const handleAction = (msg: any) => {
    logger.info(`Message View Details clicked for msg: ${msg.id}`);
    const type = msg.subtype;
    logger.info(`Type evaluated as: ${type}`);
    let targetType = '';
    if (type === 'task_due' || msg.content?.title.includes('Task')) targetType = 'task';
    else if (type === 'event_upcoming' || msg.content?.title.includes('Event')) targetType = 'calendar';
    else if (type === 'birthday_upcoming' || msg.content?.title.includes('Birthday')) targetType = 'person';
    
    const targetId = msg.content?.metadata?.target_id;
    logger.info(`Emitting open-node with target_id: ${targetId}, targetType: ${targetType}`);
    emit('open-node', targetId, targetType);
};

const getIcon = (type: string) => {
    if (type === 'task_due') return CheckSquare;
    if (type === 'event_upcoming') return Calendar;
    if (type === 'birthday_upcoming') return Gift;
    return MessageSquare;
};

const formatTime = (isoString?: string) => {
    if (!isoString) return '';
    const d = new Date(isoString);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) + ' · ' + d.toLocaleDateString();
};

defineExpose({ fetchMessages });
</script>

<template>
    <div class="flex-1 w-full h-full flex flex-col bg-gray-50 dark:bg-[#0f1115] text-text dark:text-text-dark relative overflow-hidden">
        <!-- Header -->
        <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-6 flex-shrink-0 bg-surface dark:bg-surface-dark z-10 shadow-sm" data-tauri-drag-region>
            <div class="flex items-center gap-3 font-semibold">
                <NavButtons />
                <MessageSquare class="w-5 h-5 text-blue-500" />
                <span class="text-lg tracking-tight">Chat</span>
            </div>
        </div>

        <!-- Chat Area -->
        <div 
            ref="chatContainer"
            class="flex-1 overflow-y-auto p-4 md:p-8 relative"
        >
            <div class="max-w-3xl w-full mx-auto flex flex-col gap-6 pb-8 min-h-full">
                <div v-if="loading" class="flex justify-center py-10">
                    <div class="animate-pulse flex items-center gap-2 text-gray-500">
                        <div class="w-2 h-2 bg-blue-500 rounded-full"></div>
                        <div class="w-2 h-2 bg-blue-500 rounded-full animation-delay-150"></div>
                        <div class="w-2 h-2 bg-blue-500 rounded-full animation-delay-300"></div>
                    </div>
                </div>
                
                <div v-else-if="messages.length === 0" class="flex flex-col items-center justify-center flex-1 text-gray-400 dark:text-gray-500 opacity-50 py-20">
                    <Bot class="w-16 h-16 mb-4" />
                    <h3 class="text-lg font-medium">All caught up!</h3>
                    <p class="text-sm">No new system messages.</p>
                </div>

                <template v-else>
                    <div 
                        v-for="msg in messages" 
                        :key="msg.id"
                        class="flex items-start gap-4 w-full"
                        style="animation: slideInLeft 0.3s ease-out forwards;"
                    >
                    <!-- Avatar -->
                    <div class="w-10 h-10 rounded-full bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center shadow-lg flex-shrink-0 mt-1 text-white">
                        <Bot class="w-5 h-5" />
                    </div>

                    <!-- Message Bubble -->
                    <div class="flex flex-col gap-1 min-w-0 flex-1">
                        <div class="flex items-baseline gap-2 ml-1">
                            <span class="text-sm font-semibold text-gray-700 dark:text-gray-300">{{ msg.sender?.name || 'Synabit System' }}</span>
                            <span class="text-xs text-gray-400">{{ formatTime(msg.timestamp) }}</span>
                        </div>
                        
                        <div class="bg-white dark:bg-surface-dark border border-gray-100 dark:border-border-dark shadow-sm rounded-2xl rounded-tl-none p-4 flex flex-col gap-3 relative overflow-hidden group w-full">
                            
                            <div class="flex items-start gap-3">
                                <div class="p-2 rounded-lg bg-blue-50 dark:bg-blue-900/20 text-blue-500 flex-shrink-0 mt-0.5">
                                    <component :is="getIcon(msg.subtype)" class="w-5 h-5" />
                                </div>
                                <div class="flex-1">
                                    <h4 class="font-bold text-gray-900 dark:text-white leading-tight mb-1">{{ msg.content?.title }}</h4>
                                    <p class="text-sm text-gray-600 dark:text-gray-400 whitespace-pre-wrap">{{ msg.content?.text }}</p>
                                </div>
                            </div>
                            
                            <div v-if="msg.content?.metadata?.target_id" class="border-t border-gray-100 dark:border-gray-800/60 pt-3 mt-1 flex justify-end">
                                <button 
                                    @click="handleAction(msg)"
                                    class="text-sm font-medium text-blue-600 dark:text-blue-400 hover:text-blue-700 dark:hover:text-blue-300 flex items-center gap-1.5 transition-colors"
                                >
                                    View Details
                                    <ArrowRight class="w-4 h-4" />
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            </template>
            </div>
        </div>
    </div>
</template>

<style scoped>
@keyframes slideInLeft {
    from { opacity: 0; transform: translateX(-20px); }
    to { opacity: 1; transform: translateX(0); }
}
.animation-delay-150 { animation-delay: 150ms; }
.animation-delay-300 { animation-delay: 300ms; }
</style>
