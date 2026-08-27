<script setup lang="ts">
/**
 * Naming a question about the address book so it can be asked again.
 *
 * Every field narrows the answer; within one field, any value will do. That
 * is the only rule, and saying it once here is cheaper than a screen full of
 * and/or controls nobody reads.
 */
import { ref, onMounted, onUnmounted } from 'vue';
import { X, Filter } from 'lucide-vue-next';
import { emptySegment, isEmptySegment, type Segment } from './composables/segments';
import type { HealthStatus } from './composables/useRelationshipHealth';

const props = defineProps<{
    segment: Segment | null;
    allRelationships: string[];
    allTags: string[];
}>();
const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'save', draft: Omit<Segment, 'id'>, existingId?: string): void;
    (e: 'delete', id: string): void;
}>();

const draft = ref(emptySegment());

const STATUSES: Array<{ value: HealthStatus; labelKey: string }> = [
    { value: 'overdue', labelKey: 'people.overdue' },
    { value: 'due_soon', labelKey: 'people.due_soon' },
    { value: 'on_track', labelKey: 'people.on_track' },
    { value: 'thriving', labelKey: 'people.thriving' },
    { value: 'unknown', labelKey: 'people.not_tracked' },
];

onMounted(() => {
    if (props.segment) {
        // Everything except the id, which the caller supplies.
        const { id: _ignored, ...rest } = props.segment;
        draft.value = { ...rest };
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') emit('close'); };
    window.addEventListener('keydown', onKey);
    onUnmounted(() => window.removeEventListener('keydown', onKey));
});

const toggle = (list: string[], value: string) => {
    const at = list.indexOf(value);
    if (at >= 0) list.splice(at, 1);
    else list.push(value);
};

const toggleStatus = (value: HealthStatus) => {
    const at = draft.value.statuses.indexOf(value);
    if (at >= 0) draft.value.statuses.splice(at, 1);
    else draft.value.statuses.push(value);
};

const chip = (on: boolean) =>
    on
        ? 'bg-blue-500 text-white border-blue-500'
        : 'bg-white dark:bg-[#1e1e1e] text-gray-600 dark:text-gray-400 border-border dark:border-border-dark hover:border-blue-300';
</script>

<template>
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm" @click="emit('close')">
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-2xl w-full max-w-lg max-h-[90vh] flex flex-col overflow-hidden" @click.stop>

            <div class="px-6 py-4 border-b border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <h2 class="text-lg font-semibold flex items-center gap-2">
                    <Filter class="w-5 h-5 text-blue-500" />
                    {{ segment ? $t('people.edit_segment') : $t('people.new_segment') }}
                </h2>
                <button @click="emit('close')" class="p-1.5 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 transition-colors" :aria-label="$t('people.close')">
                    <X class="w-5 h-5" />
                </button>
            </div>

            <div class="flex-1 overflow-y-auto p-6 space-y-5">
                <div>
                    <label for="segment-name" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{{ $t('people.segment_name') }}</label>
                    <input id="segment-name" v-model="draft.name" type="text" :placeholder="$t('people.segment_name_ph')"
                        class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg focus:ring-2 focus:ring-blue-500 outline-none" autofocus />
                </div>

                <div>
                    <label for="segment-query" class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{{ $t('people.contains_text') }}</label>
                    <input id="segment-query" v-model="draft.query" type="text" :placeholder="$t('people.contains_text_ph')"
                        class="w-full px-3 py-2 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none" />
                </div>

                <div v-if="allRelationships.length > 0">
                    <p class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{{ $t('people.relationships') }}</p>
                    <div class="flex flex-wrap gap-1.5">
                        <button v-for="r in allRelationships" :key="r" @click="toggle(draft.relationships, r)"
                            :class="['px-2.5 py-1 text-xs rounded-lg border transition-colors', chip(draft.relationships.includes(r))]">
                            {{ r }}
                        </button>
                    </div>
                </div>

                <div v-if="allTags.length > 0">
                    <p class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{{ $t('people.tags') }}</p>
                    <div class="flex flex-wrap gap-1.5">
                        <button v-for="t in allTags" :key="t" @click="toggle(draft.tags, t)"
                            :class="['px-2.5 py-1 text-xs rounded-lg border transition-colors', chip(draft.tags.includes(t))]">
                            {{ t }}
                        </button>
                    </div>
                </div>

                <div>
                    <p class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">{{ $t('people.keep_in_touch') }}</p>
                    <div class="flex flex-wrap gap-1.5">
                        <button v-for="s in STATUSES" :key="s.value" @click="toggleStatus(s.value)"
                            :class="['px-2.5 py-1 text-xs rounded-lg border transition-colors', chip(draft.statuses.includes(s.value))]">
                            {{ $t(s.labelKey) }}
                        </button>
                    </div>
                </div>

                <div class="flex items-center gap-2 flex-wrap">
                    <label for="segment-birthday" class="text-sm text-gray-700 dark:text-gray-300">{{ $t('people.birthday_within') }}</label>
                    <select id="segment-birthday" v-model="draft.birthdayWithinDays"
                        class="px-2.5 py-1.5 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none">
                        <option :value="null">{{ $t('people.any_time') }}</option>
                        <option :value="7">7</option>
                        <option :value="30">30</option>
                        <option :value="90">90</option>
                    </select>
                </div>
            </div>

            <div class="px-6 py-4 border-t border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <button v-if="segment" @click="emit('delete', segment.id)"
                    class="text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 px-3 py-1.5 rounded-lg transition-colors font-medium">
                    {{ $t('people.delete') }}
                </button>
                <div v-else></div>
                <button @click="emit('save', draft, segment?.id)"
                    :disabled="!draft.name.trim() || isEmptySegment(draft)"
                    class="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium text-sm transition-colors disabled:opacity-50">
                    {{ $t('people.save') }}
                </button>
            </div>
        </div>
    </div>
</template>
