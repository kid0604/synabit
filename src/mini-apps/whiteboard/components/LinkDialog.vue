<script setup lang="ts">
import { nextTick, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { cleanLink } from '../itemLinks';
import { useModalFocus } from '../composables/useModalFocus';

/** The link on an item: typed, checked, saved. */
const props = defineProps<{ link?: string }>();
const emit = defineEmits<{ (e: 'save', link: string | null): void; (e: 'close'): void }>();
const { t } = useI18n();

const text = ref(props.link ?? '');
const wrong = ref(false);
const input = ref<HTMLInputElement | null>(null);
const box = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(box, () => emit('close'));
onMounted(() => nextTick(() => { input.value?.focus(); input.value?.select(); }));

function save() {
  if (!text.value.trim()) { emit('save', null); return; }
  const link = cleanLink(text.value);
  if (!link) { wrong.value = true; return; }
  emit('save', link);
}
</script>

<template>
  <div class="wb-link-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <form ref="box" class="wb-link" role="dialog" aria-modal="true" :aria-label="t('whiteboard.link.title')" @submit.prevent="save">
      <h2 class="wb-link-title">{{ t('whiteboard.link.title') }}</h2>
      <input
        ref="input"
        v-model="text"
        class="wb-link-input"
        :placeholder="t('whiteboard.link.placeholder')"
        :aria-label="t('whiteboard.link.title')"
        :aria-invalid="wrong"
        @input="wrong = false"
      />
      <p v-if="wrong" class="wb-link-wrong" role="alert">{{ t('whiteboard.link.wrong') }}</p>
      <div class="wb-link-actions">
        <button type="button" class="wb-link-btn" @click="emit('close')">{{ t('whiteboard.close') }}</button>
        <button type="submit" class="wb-link-btn wb-link-btn--go">{{ t('whiteboard.link.save') }}</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.wb-link-backdrop { position: fixed; inset: 0; z-index: 200; display: grid; place-items: center; background: rgb(0 0 0 / 0.3); padding: 16px; }
.wb-link { width: min(420px, 100%); display: flex; flex-direction: column; gap: 10px; padding: 16px; border-radius: 14px; background: var(--color-surface, #fff); color: var(--color-text, #18181b); box-shadow: 0 16px 48px rgb(0 0 0 / 0.2); }
:global(.dark .wb-link) { background: var(--color-surface-dark, #1e1e1e); color: var(--color-text-dark, #fafafa); }
.wb-link-title { font-size: 15px; font-weight: 700; }
.wb-link-input { padding: 8px 10px; border-radius: 8px; border: 1px solid var(--color-border, #e6e6e6); background: transparent; color: inherit; font-size: 14px; }
:global(.dark .wb-link-input) { border-color: var(--color-border-dark, #333); }
.wb-link-wrong { font-size: 13px; color: var(--color-danger, #dc2626); }
.wb-link-actions { display: flex; justify-content: flex-end; gap: 8px; }
.wb-link-btn { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; }
.wb-link-btn--go { color: #fff; background: var(--color-accent); }
</style>
