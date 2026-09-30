<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import SelectionAskSyn from '../../../shared/syn/SelectionAskSyn.vue';

/** Where a selection has to be for "Ask Syn" to offer itself. */
const rootRef = ref<HTMLDivElement | null>(null);

const { t } = useI18n();

const props = defineProps<{
  filePath: string;
  vaultPath: string;
}>();

const content = ref<string | null>(null);
const isLoading = ref(true);
const error = ref('');

const extension = computed(() => {
  const parts = props.filePath.split('.');
  return parts.length > 1 ? parts.pop()!.toLowerCase() : '';
});

const isMarkdown = computed(() => extension.value === 'md');

const renderedHtml = computed(() => {
  if (!isMarkdown.value || !content.value) return '';
  return DOMPurify.sanitize(marked.parse(content.value) as string);
});

const loadContent = async () => {
  isLoading.value = true;
  error.value = '';
  try {
    content.value = await invoke<string>('read_local_file_content', {
      vaultPath: props.vaultPath,
      path: props.filePath,
    });
  } catch (e: any) {
    error.value = t('file.load_failed');
    content.value = null;
  } finally {
    isLoading.value = false;
  }
};

watch(() => props.filePath, loadContent, { immediate: true });
</script>

<template>
  <div ref="rootRef" class="flex-1 overflow-auto bg-white dark:bg-surface-dark">
    <SelectionAskSyn :within="rootRef" />
    <!-- Loading -->
    <div v-if="isLoading" class="flex items-center justify-center h-full">
      <div class="w-6 h-6 border-2 border-accent border-t-transparent rounded-full animate-spin" />
    </div>

    <!-- Error -->
    <div v-else-if="error" class="flex items-center justify-center h-full text-sm text-red-500">{{ error }}</div>

    <!-- Markdown -->
    <div v-else-if="isMarkdown" class="prose prose-sm dark:prose-invert max-w-3xl mx-auto p-8" v-html="renderedHtml" />

    <!-- Code / Plain text -->
    <pre v-else class="p-6 text-sm font-mono text-gray-800 dark:text-gray-200 whitespace-pre-wrap break-words leading-relaxed">{{ content }}</pre>
  </div>
</template>
