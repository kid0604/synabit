<script setup lang="ts">
/**
 * Keeping the current search, named on the spot.
 *
 * The first version asked for the name with `window.prompt`. That returns
 * `null` without showing anything in this app's WebView — Tauri's dialog
 * plugin has `ask`, `message`, `open` and `save` and no text prompt at all,
 * and nothing else in the codebase reaches for the browser's. So the button
 * appeared to do nothing: no dialog, no file, no error anywhere to say why.
 *
 * The name is typed in place instead, the same way the delete button asks for
 * its second press. One control, no dialog, nothing to be unimplemented.
 */
import { ref, nextTick } from 'vue';
import { Bookmark, Check, X } from 'lucide-vue-next';

const props = defineProps<{
  /** Seeds the field, so the common case is press-Enter. */
  suggestedName: string;
}>();

const emit = defineEmits<{ (e: 'save', name: string): void }>();

const naming = ref(false);
const name = ref('');
const inputRef = ref<HTMLInputElement | null>(null);

const start = async () => {
  name.value = props.suggestedName.slice(0, 40);
  naming.value = true;
  await nextTick();
  inputRef.value?.select();
};

const cancel = () => {
  naming.value = false;
  name.value = '';
};

const commit = () => {
  const trimmed = name.value.trim();
  if (!trimmed) return cancel();
  emit('save', trimmed);
  cancel();
};

defineExpose({ start, cancel });
</script>

<template>
  <button
    v-if="!naming"
    @click="start"
    class="shrink-0 flex items-center gap-1.5 px-3 py-2 rounded-full border border-gray-200 dark:border-[#2c2c2c] bg-white dark:bg-[#1e1e1e] text-xs font-medium text-gray-600 dark:text-gray-300 hover:text-black dark:hover:text-white transition-colors cursor-pointer shadow-[0_2px_8px_rgba(0,0,0,0.02)]"
    :aria-label="$t('task.a11y_save_filter')"
  >
    <Bookmark class="w-3.5 h-3.5" /> {{ $t('task.filter_save') }}
  </button>

  <div
    v-else
    class="shrink-0 flex items-center gap-1 pl-3 pr-1 py-1 rounded-full border border-blue-300 dark:border-blue-800 bg-white dark:bg-[#1e1e1e] shadow-[0_2px_8px_rgba(0,0,0,0.02)]"
  >
    <Bookmark class="w-3.5 h-3.5 shrink-0 text-blue-500" />
    <input
      ref="inputRef"
      v-model="name"
      @keydown.enter.prevent="commit"
      @keydown.escape.prevent="cancel"
      type="text"
      class="w-40 bg-transparent border-none outline-none text-xs text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400"
      :placeholder="$t('task.filter_name_prompt')"
      :aria-label="$t('task.filter_name_prompt')"
    />
    <button
      @click="commit"
      class="p-1 rounded-full text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/30 transition-colors cursor-pointer"
      :aria-label="$t('task.filter_save')"
    >
      <Check class="w-3.5 h-3.5" />
    </button>
    <button
      @click="cancel"
      class="p-1 rounded-full text-gray-400 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer"
      :aria-label="$t('task.delete_cancel')"
    >
      <X class="w-3.5 h-3.5" />
    </button>
  </div>
</template>
