<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { useModalFocus } from '../composables/useModalFocus';

/**
 * What a picture shows, in words: what a screen reader says for it, and what
 * search and Syn read. A picture used to be named by its file name, which says
 * nothing about it — "IMG_2041.jpg".
 */
const props = defineProps<{ alt?: string }>();
const emit = defineEmits<{ (e: 'save', alt: string): void; (e: 'close'): void }>();
const { t } = useI18n();

const text = ref(props.alt ?? '');
const input = ref<HTMLTextAreaElement | null>(null);
const box = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(box);
onMounted(() => nextTick(() => { input.value?.focus(); input.value?.select(); }));

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    emit('save', text.value.trim());
  }
}
</script>

<template>
  <div class="wb-alt-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <form ref="box" class="wb-alt" role="dialog" aria-modal="true" :aria-label="t('whiteboard.alt.title')" @submit.prevent="emit('save', text.trim())">
      <h2 class="wb-alt-title">{{ t('whiteboard.alt.title') }}</h2>
      <p class="wb-alt-hint">{{ t('whiteboard.alt.hint') }}</p>
      <textarea ref="input" v-model="text" class="wb-alt-input" rows="3" :aria-label="t('whiteboard.alt.title')" @keydown="onKey" />
      <div class="wb-alt-actions">
        <button type="button" class="wb-alt-btn" @click="emit('close')">{{ t('whiteboard.close') }}</button>
        <button type="submit" class="wb-alt-btn wb-alt-btn--go">{{ t('whiteboard.link.save') }}</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.wb-alt-backdrop { position: fixed; inset: 0; z-index: 200; display: grid; place-items: center; background: rgb(0 0 0 / 0.3); padding: 16px; }
.wb-alt { width: min(440px, 100%); display: flex; flex-direction: column; gap: 10px; padding: 16px; border-radius: 14px; background: var(--color-surface, #fff); color: var(--color-text, #18181b); box-shadow: 0 16px 48px rgb(0 0 0 / 0.2); }
:global(.dark .wb-alt) { background: var(--color-surface-dark, #1e1e1e); color: var(--color-text-dark, #fafafa); }
.wb-alt-title { font-size: 15px; font-weight: 700; }
.wb-alt-hint { font-size: 13px; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-alt-hint) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-alt-input { resize: vertical; padding: 8px 10px; border-radius: 8px; border: 1px solid var(--color-border, #e6e6e6); background: transparent; color: inherit; font: inherit; font-size: 14px; }
:global(.dark .wb-alt-input) { border-color: var(--color-border-dark, #333); }
.wb-alt-actions { display: flex; justify-content: flex-end; gap: 8px; }
.wb-alt-btn { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; }
.wb-alt-btn--go { color: #fff; background: var(--color-accent); }
</style>
