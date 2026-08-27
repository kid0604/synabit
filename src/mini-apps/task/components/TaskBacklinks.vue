<script setup lang="ts">
/**
 * What else in the vault points at this task.
 *
 * The panel exists to make a task feel like part of the vault rather than a
 * row in a list: the meeting note that created it, the board it was sketched
 * on, the person's page that mentions it. A hosted task manager cannot show
 * this, because it has nothing to show.
 */
import { computed } from 'vue';
import { FileText, CheckSquare, Calendar, User, Folder, PenTool, Paperclip, Link2, Loader2 } from 'lucide-vue-next';
import type { Backlink } from '../composables/useTaskBacklinks';

const props = defineProps<{ backlinks: Backlink[]; loading: boolean }>();
const emit = defineEmits<{ (e: 'open', id: string, nodeType: string): void }>();

const ICONS: Record<string, any> = {
  note: FileText, task: CheckSquare, event: Calendar,
  person: User, project: Folder, whiteboard: PenTool, file: Paperclip,
};
const iconFor = (type: string) => ICONS[type] ?? Link2;

/**
 * Grouped by what the referring thing is, then newest first inside each group.
 *
 * A flat list mixes a meeting note with a person's page and a board, and the
 * reader has to sort them out by eye. `Object.groupBy` is not Baseline yet, so
 * this does it by hand rather than reach for it.
 */
const grouped = computed(() => {
  const buckets = new Map<string, Backlink[]>();
  for (const link of props.backlinks) {
    const key = link.node_type || 'other';
    const bucket = buckets.get(key);
    if (bucket) bucket.push(link);
    else buckets.set(key, [link]);
  }
  return [...buckets.entries()].map(([type, items]) => ({ type, items }));
});
</script>

<template>
  <div class="mx-5 mb-3">
    <p class="flex items-center gap-1.5 text-[11px] font-semibold text-gray-400 dark:text-gray-500 uppercase tracking-wider mb-2">
      <Link2 class="w-3.5 h-3.5 shrink-0" />
      {{ $t('task.backlinks') }}
      <span v-if="backlinks.length" class="normal-case tracking-normal font-medium text-gray-300 dark:text-gray-600">{{ backlinks.length }}</span>
    </p>

    <div v-if="loading" class="flex items-center py-2 text-gray-400">
      <Loader2 class="w-3.5 h-3.5 animate-spin" />
    </div>

    <template v-else-if="backlinks.length">
      <div v-for="group in grouped" :key="group.type" class="mb-1.5 last:mb-0">
        <button
          v-for="link in group.items"
          :key="link.id"
          @click="emit('open', link.id, link.node_type)"
          class="w-full flex items-start gap-2 px-2 py-1.5 rounded-lg text-left hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer group"
          :aria-label="$t('task.a11y_open_backlink', { title: link.title })"
        >
          <component :is="iconFor(link.node_type)" class="w-3.5 h-3.5 mt-0.5 shrink-0 text-gray-400 group-hover:text-blue-500 transition-colors" />
          <span class="min-w-0 flex-1">
            <span class="block text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ link.title }}</span>
            <span v-if="link.preview" class="block text-[11px] text-gray-400 dark:text-gray-500 truncate">{{ link.preview }}</span>
          </span>
        </button>
      </div>
    </template>

    <p v-else class="text-[11px] text-gray-400 dark:text-gray-500 leading-relaxed">
      {{ $t('task.backlinks_none') }}
      <span class="block mt-0.5 text-gray-300 dark:text-gray-600">{{ $t('task.backlinks_hint') }}</span>
    </p>
  </div>
</template>
