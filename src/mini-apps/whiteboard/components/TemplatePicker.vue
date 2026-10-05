<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { useModalFocus } from '../composables/useModalFocus';
import { useI18n } from 'vue-i18n';
import { X } from 'lucide-vue-next';
import { TEMPLATES } from '../templates';
import { boardPreview } from '../../../shared/boardPreview';
import { newBoardData } from '../boardFile';

/**
 * Choose what a board starts with — or what to put on the open one.
 *
 * Each template is shown as the picture it makes, drawn by the same preview
 * the conversation uses for boards, so what is picked is what appears.
 */
const props = defineProps<{ mode: 'new' | 'insert' }>();
const emit = defineEmits<{ (e: 'pick', id: string): void; (e: 'close'): void }>();
const { t } = useI18n();

const dialogRef = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(dialogRef);

const choices = computed(() => {
  const list = TEMPLATES.map((tpl) => {
    const built = tpl.build((k) => t(k));
    const board = { ...newBoardData(''), nodes: built.nodes, edges: built.edges };
    return {
      id: tpl.id,
      name: t(`whiteboard.templates.${tpl.id}.name`),
      desc: t(`whiteboard.templates.${tpl.id}.desc`),
      // The preview is built from the template's own words and escapes them.
      svg: boardPreview(board),
    };
  });
  return props.mode === 'new'
    ? [{ id: 'blank', name: t('whiteboard.templates.blank'), desc: t('whiteboard.templates.blank_desc'), svg: '' }, ...list]
    : list;
});

onMounted(async () => {
  await nextTick();
  dialogRef.value?.querySelector<HTMLButtonElement>('.wb-tpl-card')?.focus();
});
</script>

<template>
  <div class="wb-tpl-backdrop" @click.self="emit('close')" @keydown.escape.stop="emit('close')" @keydown="keepFocus">
    <div ref="dialogRef" class="wb-tpl-dialog" role="dialog" aria-modal="true" :aria-label="$t('whiteboard.templates.title')">
      <div class="wb-tpl-head">
        <h2 class="wb-tpl-title">{{ $t('whiteboard.templates.title') }}</h2>
        <button class="wb-tpl-close" :aria-label="$t('whiteboard.close')" @click="emit('close')"><X :size="16" /></button>
      </div>
      <div class="wb-tpl-grid">
        <button v-for="c in choices" :key="c.id" class="wb-tpl-card" @click="emit('pick', c.id)">
          <div class="wb-tpl-thumb" aria-hidden="true">
            <!-- eslint-disable-next-line vue/no-v-html -->
            <div v-if="c.svg" class="wb-tpl-svg" v-html="c.svg" />
            <div v-else class="wb-tpl-blank" />
          </div>
          <span class="wb-tpl-name">{{ c.name }}</span>
          <span class="wb-tpl-desc">{{ c.desc }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wb-tpl-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: rgba(0, 0, 0, 0.35);
}
.wb-tpl-dialog {
  width: min(860px, 100%);
  max-height: 100%;
  overflow: auto;
  padding: 20px;
  border-radius: 16px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.25);
}
.dark .wb-tpl-dialog {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
}
.wb-tpl-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 14px;
}
.wb-tpl-title {
  font-size: 16px;
  font-weight: 700;
}
.wb-tpl-close {
  padding: 6px;
  border-radius: 8px;
}
.wb-tpl-close:hover {
  background: var(--color-surface-hover, #f4f4f5);
}
.dark .wb-tpl-close:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-tpl-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
  gap: 12px;
}
.wb-tpl-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px;
  border-radius: 12px;
  text-align: left;
  border: 1px solid var(--color-border, #e6e6e6);
  transition: border-color 0.15s, box-shadow 0.15s;
}
.dark .wb-tpl-card {
  border-color: var(--color-border-dark, #333);
}
.wb-tpl-card:hover,
.wb-tpl-card:focus-visible {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-accent) 18%, transparent);
  outline: none;
}
.wb-tpl-thumb {
  height: 110px;
  border-radius: 8px;
  background: var(--color-base, #fdfdfc);
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.dark .wb-tpl-thumb {
  background: var(--color-base-dark, #242424);
}
.wb-tpl-svg {
  width: 100%;
  height: 100%;
  padding: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.wb-tpl-svg :deep(svg) {
  max-width: 100% !important;
  max-height: 100% !important;
  width: 100%;
  height: 100%;
}
.wb-tpl-blank {
  width: 60%;
  height: 60%;
  border: 2px dashed var(--color-border, #d4d4d8);
  border-radius: 8px;
}
.wb-tpl-name {
  font-size: 13px;
  font-weight: 600;
}
.wb-tpl-desc {
  font-size: 12px;
  color: var(--color-text-secondary, #52525b);
}
.dark .wb-tpl-desc {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
</style>
