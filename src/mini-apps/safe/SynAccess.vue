<script setup lang="ts">
/**
 * What Syn may do with one item — the only place that is decided.
 *
 * Hidden (the default): Syn does not know it exists. Listed: Syn knows its
 * name and what it is for, and can remind or ask about it. Usable: Syn may
 * have it sent — only to the connectors ticked here, and only after the user
 * says yes to each one the first time. Syn never sees the value in any of the
 * three. Section 8.2 of `docs/safe-2026-09-28.md`.
 */
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import type { AiLevel, ItemView, SafeApi } from './api';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi; item: ItemView }>();
const emit = defineEmits<{ (e: 'saved', item: ItemView): void }>();
const { t } = useI18n();
const explain = useSafeError();

/** `GitHub token` → `github-token`, as the handle is suggested. */
function slug(title: string): string {
  return title
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/đ/gi, 'd')
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 40);
}

const level = ref<AiLevel>('hidden');
const handle = ref('');
const chosen = ref<string[]>([]);
const destinations = ref<{ key: string; label: string }[]>([]);
const error = ref('');
const saved = ref(false);

function reset() {
  level.value = props.item.ai_level === 'unknown' ? 'hidden' : props.item.ai_level;
  handle.value = props.item.handle ?? slug(props.item.title);
  chosen.value = [...props.item.ai_destinations];
  error.value = '';
  saved.value = false;
}
watch(() => props.item.id, reset, { immediate: true });

onMounted(async () => {
  try {
    destinations.value = await props.api.destinations();
  } catch {
    destinations.value = [];
  }
});

const dirty = computed(
  () =>
    level.value !== props.item.ai_level ||
    (level.value !== 'hidden' && handle.value !== (props.item.handle ?? '')) ||
    (level.value === 'usable' && JSON.stringify([...chosen.value].sort()) !== JSON.stringify([...props.item.ai_destinations].sort())),
);

async function save() {
  error.value = '';
  try {
    const view = await props.api.setAi(props.item.id, level.value, level.value === 'hidden' ? null : handle.value, chosen.value);
    saved.value = true;
    emit('saved', view);
  } catch (e) {
    error.value = explain(e);
  }
}

const LEVELS: AiLevel[] = ['hidden', 'listed', 'usable'];
/** What Syn writes to use it. Built here: braces inside a template's own braces end the expression early. */
const placeholder = computed(() => '{' + '{safe:' + (handle.value || '…') + '}' + '}');
</script>

<template>
  <section class="rounded-xl border border-border dark:border-border-dark p-4 space-y-3">
    <div class="flex items-center justify-between">
      <h3 class="text-sm font-semibold">{{ t('safe.ai.title') }}</h3>
    </div>
    <div class="flex gap-1 p-0.5 rounded-lg bg-surface dark:bg-surface-dark text-xs" role="radiogroup" :aria-label="t('safe.ai.title')">
      <button
        v-for="l in LEVELS"
        :key="l"
        type="button"
        role="radio"
        :aria-checked="level === l"
        class="flex-1 py-1.5 rounded-md"
        :class="level === l ? 'bg-base dark:bg-base-dark shadow-sm font-medium' : 'text-text-secondary dark:text-text-secondary-dark'"
        @click="level = l"
      >
        {{ t(`safe.ai.${l}`) }}
      </button>
    </div>
    <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t(`safe.ai.${level}_body`) }}</p>

    <template v-if="level !== 'hidden'">
      <label class="block space-y-1">
        <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ai.handle') }}</span>
        <input
          v-model="handle"
          spellcheck="false"
          autocomplete="off"
          class="w-full px-3 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
        />
        <span v-if="level === 'usable'" class="block text-xs text-text-tertiary dark:text-text-tertiary-dark">
          {{ t('safe.ai.placeholder_hint', { placeholder }) }}
        </span>
      </label>
    </template>

    <div v-if="level === 'usable'" class="space-y-1.5">
      <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ai.destinations') }}</span>
      <p v-if="!destinations.length" class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.ai.no_connectors') }}</p>
      <label v-for="d in destinations" :key="d.key" class="flex items-center gap-2 text-sm">
        <input v-model="chosen" type="checkbox" :value="d.key" />
        {{ d.label }}
      </label>
    </div>

    <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
    <div class="flex items-center gap-3">
      <button
        type="button"
        :disabled="!dirty || (level === 'usable' && !chosen.length)"
        class="px-3 py-1.5 rounded-lg bg-accent text-white text-sm font-medium disabled:opacity-40"
        @click="save"
      >
        {{ t('safe.ai.save') }}
      </button>
      <span v-if="saved && !dirty" class="text-xs text-success" role="status">{{ t('safe.ai.saved') }}</span>
    </div>
  </section>
</template>
