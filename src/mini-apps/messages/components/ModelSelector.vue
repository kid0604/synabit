<script setup lang="ts">
import { ref, computed, nextTick, watch } from 'vue';
import { ChevronDown, Check, Cpu, Download, Search, EyeOff } from 'lucide-vue-next';
import type { ModelInfo } from '../types';
import { shortlist, sizeLabel } from '../models';

const props = defineProps<{
  models: ModelInfo[];
  modelValue: string;
  formatSize: (bytes: number) => string;
  pullingModel?: boolean;
  pullProgress?: number;
  /**
   * Why the last pull stopped, when it stopped badly.
   *
   * `useSynModels` has always recorded this and nothing ever read it, so a pull
   * that failed — no disk space, no such model, Ollama gone away — looked
   * exactly like one that succeeded: the progress bar disappeared and the model
   * was simply absent from the list.
   */
  pullError?: string | null;
  /**
   * Whether pulling a model is something this provider can do at all.
   *
   * Defaults to true so the Ollama case — every case there has ever been —
   * keeps behaving exactly as it did.
   */
  canPullModels?: boolean;
}>();

const canPull = computed(() => props.canPullModels !== false);

const emit = defineEmits<{
  'update:modelValue': [value: string];
  'pull-model': [name: string];
}>();

const isOpen = ref(false);

/**
 * What is being typed, and which row the keyboard is on.
 *
 * The search box is the whole fix. Against Ollama the list is four rows and
 * nobody needed one; against a hosted endpoint it is a hundred, and hunting
 * through them by dragging a scrollbar is not a design.
 */
const query = ref('');
const cursor = ref(0);
const showAll = ref(false);
const search = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);

const filtered = computed(() =>
  shortlist(props.models, {
    query: query.value,
    selected: props.modelValue,
    showAll: showAll.value,
  })
);

/** Opening resets everything: a stale query from last time is a list that
 *  looks empty for no reason a person can see. */
const open = async () => {
  isOpen.value = !isOpen.value;
  if (!isOpen.value) return;
  query.value = '';
  showAll.value = false;
  cursor.value = 0;
  await nextTick();
  search.value?.focus();
};

// Typing moves the cursor back to the top, or it would sit past the end of a
// list that just got shorter.
watch(query, () => (cursor.value = 0));

/** Keep the highlighted row in view when the keyboard is doing the moving. */
const followCursor = async () => {
  await nextTick();
  list.value
    ?.querySelectorAll('[data-model-row]')
    [cursor.value]?.scrollIntoView({ block: 'nearest' });
};

const onKeydown = async (e: KeyboardEvent) => {
  const rows = filtered.value.shown;
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    cursor.value = rows.length ? (cursor.value + 1) % rows.length : 0;
    await followCursor();
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    cursor.value = rows.length ? (cursor.value - 1 + rows.length) % rows.length : 0;
    await followCursor();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    const picked = rows[cursor.value];
    if (picked) selectModel(picked.name);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    // Stopped here rather than left to bubble: the composer and the run both
    // listen for Escape, and closing a dropdown must not also stop a stream.
    e.stopPropagation();
    isOpen.value = false;
  }
};

const selectedModelInfo = computed(() => {
  return props.models.find(m => m.name === props.modelValue);
});

const displayName = computed(() => {
  if (!selectedModelInfo.value) return '';
  const name = selectedModelInfo.value.name;
  // Show short name: e.g., "gemma2:7b" → "Gemma2 7B"
  return name.split(':').map(p => p.charAt(0).toUpperCase() + p.slice(1)).join(' ');
});

const selectModel = (name: string) => {
  emit('update:modelValue', name);
  isOpen.value = false;
};

const handleClickOutside = () => {
  isOpen.value = false;
};

const customModelName = ref('');
const pendingPullName = ref<string | null>(null);

const handlePullCustom = () => {
  const name = customModelName.value.trim();
  if (!name) return;
  pendingPullName.value = name;
};

const confirmPull = () => {
  if (!pendingPullName.value) return;
  emit('pull-model', pendingPullName.value);
  customModelName.value = '';
  pendingPullName.value = null;
};

const cancelConfirm = () => {
  pendingPullName.value = null;
};
</script>

<template>
  <div class="relative" v-if="models.length > 0">
    <!-- Trigger -->
    <button
      @click.stop="open()"
      class="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-white/60 dark:bg-white/5 border border-border dark:border-border-dark hover:bg-white dark:hover:bg-white/10 transition-all cursor-pointer text-sm"
    >
      <Cpu class="w-3.5 h-3.5 text-violet-500" />
      <span class="font-medium text-text dark:text-text-dark max-w-[140px] truncate">
        {{ displayName || $t('syn.select_model') }}
      </span>
      <span
        v-if="selectedModelInfo?.details?.parameter_size"
        class="text-xs text-gray-400 dark:text-gray-500"
      >
        {{ selectedModelInfo.details.parameter_size }}
      </span>
      <ChevronDown class="w-3.5 h-3.5 text-gray-400 transition-transform" :class="{ 'rotate-180': isOpen }" />
    </button>

    <!-- Dropdown overlay -->
    <div v-if="isOpen" class="fixed inset-0 z-40" @click="handleClickOutside" />

    <!-- Dropdown -->
    <Transition
      enter-active-class="transition ease-out duration-150"
      enter-from-class="opacity-0 scale-95 -translate-y-1"
      enter-to-class="opacity-100 scale-100 translate-y-0"
      leave-active-class="transition ease-in duration-100"
      leave-from-class="opacity-100 scale-100 translate-y-0"
      leave-to-class="opacity-0 scale-95 -translate-y-1"
    >
      <div
        v-if="isOpen"
        class="absolute right-0 top-full mt-2 w-72 bg-white dark:bg-[#1a1a1f] border border-border dark:border-border-dark rounded-xl shadow-xl z-50 overflow-hidden"
      >
        <!-- Type to find one. The box is the fix: a hundred rows and a
             scrollbar is not a way to choose anything. -->
        <div class="p-2 border-b border-border dark:border-border-dark">
          <div class="flex items-center gap-2 px-2">
            <Search class="w-3.5 h-3.5 text-gray-400 flex-shrink-0" />
            <input
              ref="search"
              v-model="query"
              type="text"
              :placeholder="$t('syn.model_search')"
              class="flex-1 min-w-0 bg-transparent text-sm text-text dark:text-text-dark
                     placeholder-gray-400 dark:placeholder-gray-500 outline-none py-1"
              @keydown="onKeydown"
            >
            <span class="text-[11px] text-gray-400 tabular-nums flex-shrink-0">
              {{ filtered.shown.length }}
            </span>
          </div>
        </div>

        <div ref="list" class="max-h-64 overflow-y-auto p-1">
          <button
            v-for="(model, i) in filtered.shown"
            :key="model.name"
            data-model-row
            @click="selectModel(model.name)"
            @mousemove="cursor = i"
            class="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left transition-colors cursor-pointer"
            :class="model.name === modelValue
              ? 'bg-violet-50 dark:bg-violet-500/10 text-violet-700 dark:text-violet-300'
              : i === cursor
                ? 'bg-gray-100 dark:bg-white/10 text-text dark:text-text-dark'
                : 'text-text dark:text-text-dark'"
          >
            <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-violet-500/10 to-purple-500/10 dark:from-violet-500/20 dark:to-purple-500/20 flex items-center justify-center flex-shrink-0">
              <Cpu class="w-4 h-4 text-violet-500" />
            </div>
            <div class="flex-1 min-w-0">
              <div class="font-medium text-sm truncate">{{ model.name }}</div>
              <!-- Only when there is something to say. A hosted endpoint sends
                   no size, and `0 MB` under every row reads as a measurement. -->
              <div
                v-if="sizeLabel(model, formatSize) || model.details?.family"
                class="flex items-center gap-2 text-xs text-gray-400 dark:text-gray-500"
              >
                <span v-if="sizeLabel(model, formatSize)">{{ sizeLabel(model, formatSize) }}</span>
                <span v-if="model.details?.family">{{ model.details.family }}</span>
              </div>
            </div>
            <Check v-if="model.name === modelValue" class="w-4 h-4 text-violet-500 flex-shrink-0" />
          </button>

          <p
            v-if="!filtered.shown.length"
            class="px-3 py-6 text-center text-[13px] text-gray-400"
          >
            {{ $t('syn.model_none_match', { query }) }}
          </p>
        </div>

        <!-- What was left out, and the way back to it. Nothing is hidden
             outright: the rule is a guess about a name, and a guess that
             cannot be undone is not one worth making. -->
        <button
          v-if="filtered.hidden"
          @click="showAll = !showAll"
          class="w-full flex items-center gap-1.5 px-3 py-2 border-t border-border dark:border-border-dark
                 text-[11px] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300
                 transition-colors cursor-pointer"
        >
          <EyeOff class="w-3 h-3 flex-shrink-0" />
          {{ $t('syn.model_hidden', { n: filtered.hidden }) }}
        </button>

        <!-- Pull progress (shown when pulling) -->
        <div v-if="pullingModel" class="px-3 py-2 border-t border-border dark:border-border-dark">
          <div class="w-full bg-gray-100 dark:bg-gray-800 rounded-full h-1.5 overflow-hidden">
            <div
              class="h-full bg-gradient-to-r from-violet-500 to-purple-600 rounded-full transition-all duration-300"
              :style="{ width: (pullProgress || 0) + '%' }"
            />
          </div>
          <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-1 text-center">
            {{ $t('syn.pulling_model') }} {{ Math.round(pullProgress || 0) }}%
          </p>
        </div>

        <!--
          Why the last pull failed. Not shown while one is in flight: `pullError`
          is cleared when a pull starts, but showing both at once would still
          read as the new attempt having already failed.
        -->
        <div v-else-if="pullError" class="px-3 py-2 border-t border-border dark:border-border-dark">
          <p class="text-[11px] text-red-500 dark:text-red-400 text-center">
            {{ $t('syn.pull_failed') }}
          </p>
          <p class="text-[10px] text-gray-400 dark:text-gray-500 mt-0.5 text-center break-words">
            {{ pullError }}
          </p>
        </div>

        <!-- Pull new model -->
        <div v-if="canPull" class="p-2 border-t border-border dark:border-border-dark">
          <!-- Confirm step -->
          <div v-if="pendingPullName" class="space-y-2">
            <p class="text-xs text-text dark:text-text-dark px-1">
              {{ $t('syn.confirm_pull') }}
              <span class="font-semibold text-violet-600 dark:text-violet-400">{{ pendingPullName }}</span>
            </p>
            <div class="flex items-center gap-1.5">
              <button
                @click.stop="confirmPull"
                class="flex-1 px-2.5 py-1.5 text-xs rounded-lg bg-violet-500 hover:bg-violet-600 text-white font-medium transition-colors cursor-pointer"
              >
                {{ $t('syn.confirm_pull_btn') }}
              </button>
              <button
                @click.stop="cancelConfirm"
                class="px-2.5 py-1.5 text-xs rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
              >
                {{ $t('syn.cancel') }}
              </button>
            </div>
          </div>
          <!-- Input step -->
          <div v-else class="flex items-center gap-1.5">
            <input
              v-model="customModelName"
              :disabled="pullingModel"
              @keydown.enter="handlePullCustom"
              :placeholder="$t('syn.custom_model_placeholder')"
              class="flex-1 px-2.5 py-1.5 text-xs bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/60 rounded-lg text-text dark:text-text-dark placeholder-gray-400 dark:placeholder-gray-500 outline-none focus:border-violet-400 dark:focus:border-violet-500/50 transition-colors disabled:opacity-50"
              @click.stop
            />
            <button
              @click.stop="handlePullCustom"
              :disabled="!customModelName.trim() || pullingModel"
              class="p-1.5 rounded-lg transition-all cursor-pointer flex-shrink-0"
              :class="customModelName.trim() && !pullingModel
                ? 'bg-violet-500 hover:bg-violet-600 text-white'
                : 'bg-gray-100 dark:bg-gray-800 text-gray-400 dark:text-gray-500 cursor-not-allowed'"
             aria-label="Handle Pull Custom">
              <Download class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>
