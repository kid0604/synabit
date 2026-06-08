<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue';
import { X, Gift, ArrowUpRight, ArrowDownLeft } from 'lucide-vue-next';

defineProps<{
    person: any;
}>();

const emit = defineEmits(['close', 'save']);

const form = ref({
    description: '',
    date: new Date().toISOString().split('T')[0],
    direction: 'given' as 'given' | 'received',
    occasion: '',
});

const save = () => {
    if (!form.value.description.trim()) return;
    emit('save', {
        id: crypto.randomUUID(),
        description: form.value.description.trim(),
        date: form.value.date,
        direction: form.value.direction,
        occasion: form.value.occasion.trim() || undefined,
    });
    emit('close');
};

onMounted(() => {
    const handleKeydown = (e: KeyboardEvent) => {
        if (e.key === 'Escape') emit('close');
    };
    window.addEventListener('keydown', handleKeydown);
    onUnmounted(() => window.removeEventListener('keydown', handleKeydown));
});
</script>

<template>
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm" @click="emit('close')">
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-2xl w-full max-w-md flex flex-col overflow-hidden" @click.stop>
            <!-- Header -->
            <div class="px-5 py-4 border-b border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <h2 class="text-base font-semibold flex items-center gap-2">
                    <Gift class="w-4 h-4 text-pink-500" />
                    Log Gift — {{ person.title }}
                </h2>
                <button @click="emit('close')" class="p-1.5 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 transition-colors">
                    <X class="w-4 h-4" />
                </button>
            </div>

            <div class="p-5 space-y-4">
                <!-- Direction Toggle -->
                <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">{{ $t('people.direction') }}</label>
                    <div class="flex gap-2">
                        <button
                            @click="form.direction = 'given'"
                            :class="['flex-1 flex items-center justify-center gap-2 py-2.5 rounded-lg text-sm font-medium transition-all border-2',
                                form.direction === 'given'
                                    ? 'bg-blue-50 dark:bg-blue-900/20 border-blue-500 text-blue-700 dark:text-blue-300'
                                    : 'bg-gray-50 dark:bg-gray-800 border-gray-200 dark:border-gray-700 text-gray-500 hover:border-gray-300'
                            ]"
                        >
                            <ArrowUpRight class="w-4 h-4" /> I gave
                        </button>
                        <button
                            @click="form.direction = 'received'"
                            :class="['flex-1 flex items-center justify-center gap-2 py-2.5 rounded-lg text-sm font-medium transition-all border-2',
                                form.direction === 'received'
                                    ? 'bg-green-50 dark:bg-green-900/20 border-green-500 text-green-700 dark:text-green-300'
                                    : 'bg-gray-50 dark:bg-gray-800 border-gray-200 dark:border-gray-700 text-gray-500 hover:border-gray-300'
                            ]"
                        >
                            <ArrowDownLeft class="w-4 h-4" /> I received
                        </button>
                    </div>
                </div>

                <!-- Description -->
                <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ $t('people.gift_name') }}</label>
                    <input v-model="form.description" type="text" :placeholder="$t('people.gift_ph')" autofocus
                        class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 outline-none text-sm" />
                </div>

                <!-- Date + Occasion -->
                <div class="grid grid-cols-2 gap-3">
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ $t('people.date') }}</label>
                        <input v-model="form.date" type="date"
                            class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 outline-none text-sm" />
                    </div>
                    <div>
                        <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ $t('people.occasion') }}</label>
                        <input v-model="form.occasion" type="text" :placeholder="$t('people.occasion_ph')"
                            class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 outline-none text-sm" />
                    </div>
                </div>
            </div>

            <!-- Footer -->
            <div class="px-5 py-4 border-t border-border dark:border-border-dark flex justify-end gap-3 bg-gray-50/50 dark:bg-gray-800/50">
                <button @click="emit('close')" class="px-4 py-2 text-sm text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-lg transition-colors">{{ $t('people.cancel') }}</button>
                <button @click="save" :disabled="!form.description.trim()" class="px-4 py-2 text-sm bg-pink-500 text-white rounded-lg hover:bg-pink-600 transition-colors disabled:opacity-50 font-medium flex items-center gap-1.5">
                    <Gift class="w-3.5 h-3.5" /> Log Gift
                </button>
            </div>
        </div>
    </div>
</template>
