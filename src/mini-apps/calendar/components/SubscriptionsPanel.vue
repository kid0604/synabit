<script setup lang="ts">
import { ref } from 'vue';
import { X, RefreshCw, Plus, Trash2, AlertCircle, Bell, BellOff } from 'lucide-vue-next';
import type { Subscription } from '../subscriptions';
import { paletteFor } from '../subscriptions';
import AppDialog from '../../../shared/components/AppDialog.vue';

defineProps<{
    show: boolean;
    subscriptions: Subscription[];
    busy: boolean;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'add', url: string, name: string): void;
    (e: 'remove', id: string): void;
    (e: 'toggle', id: string, enabled: boolean): void;
    (e: 'toggle-remind', id: string, remind: boolean): void;
    (e: 'refresh'): void;
}>();

const url = ref('');
const name = ref('');

const submit = () => {
    if (!url.value.trim()) return;
    emit('add', url.value, name.value);
    url.value = '';
    name.value = '';
};

const when = (seconds: number) =>
    seconds > 0 ? new Date(seconds * 1000).toLocaleString() : '';

const field = 'w-full bg-gray-50 dark:bg-surface-hover-dark border border-gray-200 dark:border-[#444] '
    + 'rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-accent text-black dark:text-white';
</script>

<template>
    <AppDialog :show="show" labelledby="subs-title" size="md" unstyled
               panel-class="bg-white dark:bg-surface-dark rounded-2xl shadow-2xl overflow-hidden border border-border dark:border-[#333] flex flex-col max-h-[85vh]"
               @close="emit('close')">
            <div class="flex items-center justify-between px-6 py-4 border-b border-border dark:border-[#333]">
                <h3 id="subs-title" class="font-bold text-lg text-black dark:text-white">
                    {{ $t('calendar.subscriptions') }}
                </h3>
                <div class="flex items-center gap-1">
                    <button @click="emit('refresh')" :disabled="busy"
                            :aria-label="$t('calendar.subscribe_refresh')" :title="$t('calendar.subscribe_refresh')"
                            class="p-1.5 rounded-md text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 disabled:opacity-40 transition-colors">
                        <RefreshCw class="w-4 h-4" :class="busy ? 'animate-spin' : ''" />
                    </button>
                    <button @click="emit('close')" :aria-label="$t('calendar.a11y_close')" :title="$t('calendar.a11y_close')"
                            class="p-1.5 rounded-md text-gray-500 dark:text-gray-400 hover:text-red-500 transition-colors">
                        <X class="w-4 h-4" />
                    </button>
                </div>
            </div>

            <div class="px-6 py-4 space-y-3 overflow-y-auto">
                <p v-if="subscriptions.length === 0" class="text-[13px] text-gray-500 dark:text-gray-400 italic">
                    {{ $t('calendar.subscribe_none') }}
                </p>

                <div v-for="sub in subscriptions" :key="sub.id"
                     class="flex items-start gap-3 p-3 rounded-xl border border-gray-100 dark:border-[#333] bg-gray-50/60 dark:bg-base-dark">
                    <span class="w-3 h-3 rounded-full mt-1 shrink-0" :class="paletteFor(sub.colour).swatch"></span>
                    <div class="flex-1 min-w-0">
                        <p class="text-[13px] font-semibold truncate text-text dark:text-text-dark">{{ sub.name }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 truncate" :title="sub.url">{{ sub.url }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                            {{ $t('calendar.subscribe_events_n', { n: sub.eventCount }) }}
                            <span v-if="sub.lastFetchedAt"> · {{ when(sub.lastFetchedAt) }}</span>
                            <span v-else> · {{ $t('calendar.subscribe_never') }}</span>
                        </p>
                        <p v-if="sub.lastError" class="flex items-start gap-1 text-xs text-red-500 mt-1">
                            <AlertCircle class="w-3 h-3 mt-0.5 shrink-0" />{{ sub.lastError }}
                        </p>
                    </div>
                    <div class="flex items-center gap-1 shrink-0">
                        <!-- Off by default: a holidays feed would otherwise
                             announce every holiday at midnight. -->
                        <button type="button" @click="emit('toggle-remind', sub.id, !sub.remind)"
                                :aria-pressed="sub.remind"
                                :aria-label="sub.remind ? $t('calendar.subscribe_remind') : $t('calendar.subscribe_remind_off')"
                                :title="sub.remind ? $t('calendar.subscribe_remind') : $t('calendar.subscribe_remind_off')"
                                class="p-1.5 rounded-md transition-colors"
                                :class="sub.remind ? 'text-accent dark:text-accent-dark hover:bg-accent/10 dark:hover:bg-accent/15' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-surface-hover-dark'">
                            <Bell v-if="sub.remind" class="w-3.5 h-3.5" />
                            <BellOff v-else class="w-3.5 h-3.5" />
                        </button>
                        <button role="switch" :aria-checked="sub.enabled"
                                :aria-label="$t('calendar.subscribe_show')"
                                @click="emit('toggle', sub.id, !sub.enabled)"
                                class="relative w-9 h-5 rounded-full transition-colors"
                                :class="sub.enabled ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'">
                            <span class="absolute top-0.5 w-4 h-4 rounded-full bg-white transition-all"
                                  :class="sub.enabled ? 'left-[18px]' : 'left-0.5'"></span>
                        </button>
                        <button @click="emit('remove', sub.id)" :aria-label="$t('calendar.subscribe_remove')"
                                class="p-1.5 rounded-md text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">
                            <Trash2 class="w-3.5 h-3.5" />
                        </button>
                    </div>
                </div>
            </div>

            <form class="px-6 py-4 border-t border-border dark:border-[#333] bg-gray-50 dark:bg-[#1a1a1a] flex flex-col gap-2"
                  @submit.prevent="submit">
                <label class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider" for="sub-url">
                    {{ $t('calendar.subscribe_add') }}
                </label>
                <input id="sub-url" v-model="url" type="url" inputmode="url"
                       :placeholder="$t('calendar.subscribe_url_ph')" :class="field">
                <div class="flex items-center gap-2">
                    <input v-model="name" type="text" :placeholder="$t('calendar.subscribe_name_ph')"
                           :aria-label="$t('calendar.subscribe_name_ph')" :class="field">
                    <button type="submit" :disabled="busy || !url.trim()"
                            class="btn-primary shrink-0 disabled:opacity-40">
                        <Plus class="w-3.5 h-3.5" />{{ $t('calendar.subscribe_add') }}
                    </button>
                </div>
            </form>
    </AppDialog>
</template>
