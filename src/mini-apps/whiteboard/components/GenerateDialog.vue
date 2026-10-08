<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { FileText, X } from 'lucide-vue-next';
import VaultPicker from './VaultPicker.vue';
import { useModalFocus } from '../composables/useModalFocus';

/**
 * Ask Syn for a diagram: what it should show, in words, and what to draw it
 * from — the items selected on the board, a note from the vault, or both.
 */
const props = defineProps<{ selectionCount: number }>();
const emit = defineEmits<{
  (e: 'submit', value: { request: string; useSelection: boolean; source?: { id: string; title: string } }): void;
  (e: 'close'): void;
}>();
const { t } = useI18n();

const request = ref('');
const useSelection = ref(props.selectionCount > 0);
const source = ref<{ id: string; title: string } | null>(null);
const picking = ref(false);
const input = ref<HTMLTextAreaElement | null>(null);
const box = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(box, () => emit('close'));
onMounted(() => nextTick(() => input.value?.focus()));

const ready = computed(() => request.value.trim().length > 0);

function submit() {
  if (!ready.value) return;
  emit('submit', {
    request: request.value.trim(),
    useSelection: useSelection.value && props.selectionCount > 0,
    ...(source.value ? { source: source.value } : {}),
  });
}

function onPick(item: { id: string; title: string }) {
  source.value = { id: item.id, title: item.title };
  picking.value = false;
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    submit();
  }
}
</script>

<template>
  <div class="wb-gen-backdrop" @click.self="emit('close')" @keydown.escape.stop="!picking && emit('close')" @keydown="keepFocus">
    <form ref="box" class="wb-gen" role="dialog" aria-modal="true" :aria-label="t('whiteboard.generate.title')" @submit.prevent="submit">
      <h2 class="wb-gen-title">{{ t('whiteboard.generate.title') }}</h2>
      <textarea
        ref="input"
        v-model="request"
        class="wb-gen-input"
        rows="3"
        :placeholder="t('whiteboard.generate.placeholder')"
        :aria-label="t('whiteboard.generate.title')"
        @keydown="onKey"
      />
      <p class="wb-gen-label">{{ t('whiteboard.generate.from') }}</p>
      <label v-if="selectionCount > 0" class="wb-gen-check">
        <input v-model="useSelection" type="checkbox" />
        <span>{{ t('whiteboard.generate.use_selection', { count: selectionCount }) }}</span>
      </label>
      <div class="wb-gen-source">
        <span v-if="source" class="wb-gen-chip">
          <FileText :size="13" aria-hidden="true" />
          <span class="wb-gen-chip-text">{{ source.title }}</span>
          <button type="button" class="wb-gen-chip-x" :aria-label="t('whiteboard.generate.remove_source')" @click="source = null"><X :size="12" /></button>
        </span>
        <button v-else type="button" class="wb-gen-btn" @click="picking = true">{{ t('whiteboard.generate.pick_source') }}</button>
      </div>
      <div class="wb-gen-actions">
        <button type="button" class="wb-gen-btn" @click="emit('close')">{{ t('whiteboard.close') }}</button>
        <button type="submit" class="wb-gen-btn wb-gen-btn--go" :disabled="!ready" :title="t('whiteboard.generate.go_hint')">{{ t('whiteboard.generate.go') }}</button>
      </div>
    </form>
    <VaultPicker
      v-if="picking"
      @pick="onPick"
      @close="picking = false"
    />
  </div>
</template>

<style scoped>
.wb-gen-backdrop { position: fixed; inset: 0; z-index: 200; display: grid; place-items: center; background: rgb(0 0 0 / 0.3); padding: 16px; }
.wb-gen { width: min(520px, 100%); display: flex; flex-direction: column; gap: 10px; padding: 16px; border-radius: 14px; background: var(--color-surface, #fff); color: var(--color-text, #18181b); box-shadow: 0 16px 48px rgb(0 0 0 / 0.2); }
:global(.dark .wb-gen) { background: var(--color-surface-dark, #1e1e1e); color: var(--color-text-dark, #fafafa); }
.wb-gen-title { font-size: 15px; font-weight: 700; }
.wb-gen-input { resize: vertical; min-height: 72px; padding: 8px 10px; border-radius: 8px; border: 1px solid var(--color-border, #e6e6e6); background: transparent; color: inherit; font: inherit; font-size: 14px; }
:global(.dark .wb-gen-input) { border-color: var(--color-border-dark, #333); }
.wb-gen-label { font-size: 12px; font-weight: 600; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-gen-label) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-gen-check { display: flex; align-items: center; gap: 8px; font-size: 13px; }
.wb-gen-source { display: flex; }
.wb-gen-chip { display: inline-flex; align-items: center; gap: 6px; max-width: 100%; padding: 4px 6px 4px 10px; border-radius: 999px; font-size: 13px; background: color-mix(in oklab, var(--color-accent) 12%, transparent); }
.wb-gen-chip-text { overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.wb-gen-chip-x { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 999px; }
.wb-gen-actions { display: flex; justify-content: flex-end; gap: 8px; }
.wb-gen-btn { padding: 6px 14px; border-radius: 8px; font-size: 13px; font-weight: 600; border: 1px solid var(--color-border, #e6e6e6); }
:global(.dark .wb-gen-btn) { border-color: var(--color-border-dark, #333); }
.wb-gen-btn--go { color: #fff; border-color: transparent; background: var(--color-accent); }
.wb-gen-btn--go:disabled { opacity: 0.4; }
</style>
