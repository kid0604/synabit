<script setup lang="ts">
/**
 * "What to do with these 3 PDFs?" — files first, tool second (design §5.5).
 *
 * Offered after files are dropped on the home screen or chosen with its
 * "Choose files…" button. Only tools that take this kind and this many files
 * are listed. Arrows move, Enter opens, a digit opens that row.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { FileText, Image as ImageIcon } from 'lucide-vue-next';
import AppDialog from '../../../shared/components/AppDialog.vue';
import { availability, suggestionsFor, type InputKind, type ToolId } from '../tools';
import { useWorkbench } from '../useWorkbench';
import type { Input } from '../api';

const props = defineProps<{ show: boolean; kind: InputKind | null; files: Input[]; leftOut: number }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'choose', tool: ToolId): void }>();

const { t } = useI18n();
const active = ref(0);
const wb = useWorkbench();
/** Tools that cannot run here are not offered: a drop is for doing something now. */
const tools = computed(() => (props.kind ? suggestionsFor(props.kind, props.files.length).filter((t) => availability(t, wb.vision.value, wb.office.value) === 'ready') : []));
watch(() => props.show, () => (active.value = 0));

function onKey(e: KeyboardEvent) {
  if (!tools.value.length) return;
  if (e.key === 'ArrowDown') {
    active.value = (active.value + 1) % tools.value.length;
    e.preventDefault();
  } else if (e.key === 'ArrowUp') {
    active.value = (active.value - 1 + tools.value.length) % tools.value.length;
    e.preventDefault();
  } else if (e.key === 'Enter') {
    emit('choose', tools.value[active.value].id);
    e.preventDefault();
  } else if (/^[1-9]$/.test(e.key) && !e.metaKey && !e.ctrlKey) {
    const tool = tools.value[Number(e.key) - 1];
    if (tool) emit('choose', tool.id);
  }
}
</script>

<template>
  <AppDialog :show="show" labelledby="utility-drop-title" size="md" @close="emit('close')">
    <div class="p-1" @keydown="onKey">
      <h2 id="utility-drop-title" class="text-base font-semibold text-text dark:text-text-dark m-0 px-3 pt-2">
        {{ kind ? t(`utilities.smart_drop.title_${kind}`, { n: files.length }, files.length) : t('utilities.smart_drop.nothing') }}
      </h2>
      <ul class="flex flex-wrap gap-1.5 px-3 pt-3 pb-2 list-none m-0">
        <li v-for="f in files.slice(0, 6)" :key="f.id" class="inline-flex items-center gap-1.5 text-[12.5px] px-2.5 py-0.5 rounded-full bg-surface-hover dark:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark max-w-[220px]">
          <ImageIcon v-if="kind === 'image'" class="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
          <FileText v-else class="w-3.5 h-3.5 shrink-0" aria-hidden="true" />
          <span class="truncate">{{ f.name }}</span>
        </li>
        <li v-if="files.length > 6" class="text-[12.5px] px-2 py-0.5 text-muted">+{{ files.length - 6 }}</li>
      </ul>
      <p v-if="leftOut" class="text-xs text-muted dark:text-muted-dark px-3 m-0 pb-1">{{ t('utilities.smart_drop.left_out', { n: leftOut }, leftOut) }}</p>

      <ul class="list-none m-0 p-1" role="listbox" :aria-labelledby="'utility-drop-title'">
        <li v-for="(tool, i) in tools" :key="tool.id" role="option" :aria-selected="i === active">
          <button
            type="button"
            class="w-full flex items-center gap-3 px-2.5 py-2 rounded-lg text-left"
            :class="i === active ? 'bg-surface-hover dark:bg-surface-hover-dark' : 'hover:bg-surface-hover dark:hover:bg-surface-hover-dark'"
            @mouseenter="active = i"
            @focus="active = i"
            @click="emit('choose', tool.id)"
          >
            <span class="w-8 h-8 rounded-lg grid place-items-center shrink-0" :class="`utility-tint-${tool.category}`">
              <component :is="tool.icon" class="w-4 h-4" aria-hidden="true" />
            </span>
            <span class="flex-1 min-w-0">
              <span class="block font-semibold text-sm text-text dark:text-text-dark">{{ t(`utilities.tools.${tool.id}.name`) }}</span>
              <span class="block text-[12.5px] text-text-secondary dark:text-text-secondary-dark truncate">{{ t(`utilities.tools.${tool.id}.desc`) }}</span>
            </span>
            <kbd v-if="i < 9" class="text-xs text-muted border border-border dark:border-border-dark rounded px-1.5">{{ i + 1 }}</kbd>
          </button>
        </li>
      </ul>
      <p v-if="kind && !tools.length" class="text-sm text-text-secondary px-3 pb-3 m-0">{{ t('utilities.smart_drop.none_fit') }}</p>
      <p class="border-t border-border dark:border-border-dark mt-1 px-3 pt-2.5 pb-1 text-xs text-muted dark:text-muted-dark m-0">{{ t('utilities.smart_drop.keys') }}</p>
    </div>
  </AppDialog>
</template>
