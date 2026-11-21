<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import { useModalFocus } from '../composables/useModalFocus';
import { useI18n } from 'vue-i18n';
import { Search, X } from 'lucide-vue-next';
import { TEMPLATE_CATEGORIES, TEMPLATES, type TemplateCategory } from '../templates';
import { boardPreview } from '../../../shared/boardPreview';
import { newBoardData } from '../boardFile';

/**
 * Choose what a board starts with — or what to put on the open one.
 *
 * Each template is shown as the picture it makes, drawn by the same preview
 * the conversation uses for boards, so what is picked is what appears. With
 * forty-odd of them they come in groups, as other whiteboards show theirs:
 * all of them under their group's name, one group alone, or those whose name
 * or description has the words typed.
 */
const props = defineProps<{ mode: 'new' | 'insert' }>();
const emit = defineEmits<{ (e: 'pick', id: string): void; (e: 'close'): void }>();
const { t } = useI18n();

const dialogRef = ref<HTMLElement | null>(null);
const keepFocus = useModalFocus(dialogRef, () => emit('close'));

/** The group shown, or every group. */
const GROUPS = ['all', ...TEMPLATE_CATEGORIES] as const;
const category = ref<(typeof GROUPS)[number]>('all');
const query = ref('');

interface Choice { id: string; category?: TemplateCategory; name: string; desc: string; svg: string }

// Built once, when the picker opens: the words do not change while it is open.
const all: Choice[] = TEMPLATES.map((tpl) => {
  const built = tpl.build((k) => t(k));
  const board = { ...newBoardData(''), nodes: built.nodes, edges: built.edges };
  return {
    id: tpl.id,
    category: tpl.category,
    name: t(`whiteboard.templates.${tpl.id}.name`),
    desc: t(`whiteboard.templates.${tpl.id}.desc`),
    // The preview is built from the template's own words and escapes them.
    svg: boardPreview(board),
  };
});
const blank: Choice = { id: 'blank', name: t('whiteboard.templates.blank'), desc: t('whiteboard.templates.blank_desc'), svg: '' };

/** Words compared without case or accents: "so do" finds "Sơ đồ". */
const fold = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').replace(/đ/g, 'd').replace(/Đ/g, 'D').toLowerCase();

/** What is shown: in groups with their names, or one list when narrowed. */
const sections = computed<{ key: string; title: string; items: Choice[] }[]>(() => {
  const words = fold(query.value.trim());
  if (words) {
    const items = all.filter((c) => fold(`${c.name} ${c.desc}`).includes(words));
    return items.length ? [{ key: 'found', title: '', items }] : [];
  }
  if (category.value !== 'all') {
    return [{ key: category.value, title: '', items: all.filter((c) => c.category === category.value) }];
  }
  const groups = TEMPLATE_CATEGORIES.map((key) => ({
    key, title: t(`whiteboard.templates.categories.${key}`), items: all.filter((c) => c.category === key),
  }));
  return props.mode === 'new' ? [{ key: 'blank', title: '', items: [blank] }, ...groups] : groups;
});

function choose(key: TemplateCategory | 'all') {
  category.value = key;
  query.value = '';
}

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
        <label class="wb-tpl-search">
          <Search :size="14" aria-hidden="true" />
          <input
            v-model="query"
            type="search"
            :placeholder="$t('whiteboard.templates.search')"
            :aria-label="$t('whiteboard.templates.search')"
          />
        </label>
        <button class="wb-tpl-close" :aria-label="$t('whiteboard.close')" @click="emit('close')"><X :size="16" /></button>
      </div>
      <div class="wb-tpl-groups" role="group" :aria-label="$t('whiteboard.templates.title')">
        <button
          v-for="key in GROUPS"
          :key="key"
          type="button"
          class="wb-tpl-group"
          :class="{ active: category === key && !query }"
          :aria-pressed="category === key && !query"
          @click="choose(key)"
        >{{ key === 'all' ? $t('whiteboard.templates.all') : $t(`whiteboard.templates.categories.${key}`) }}</button>
      </div>
      <div class="wb-tpl-body">
        <section v-for="s in sections" :key="s.key" class="wb-tpl-section">
          <h3 v-if="s.title" class="wb-tpl-section-title">{{ s.title }}</h3>
          <div class="wb-tpl-grid">
            <button v-for="c in s.items" :key="c.id" class="wb-tpl-card" @click="emit('pick', c.id)">
              <div class="wb-tpl-thumb" aria-hidden="true">
                <!-- eslint-disable-next-line vue/no-v-html -->
                <div v-if="c.svg" class="wb-tpl-svg" v-html="c.svg" />
                <div v-else class="wb-tpl-blank" />
              </div>
              <span class="wb-tpl-name">{{ c.name }}</span>
              <span class="wb-tpl-desc">{{ c.desc }}</span>
            </button>
          </div>
        </section>
        <p v-if="!sections.length" class="wb-tpl-empty">{{ $t('whiteboard.templates.no_match', { q: query.trim() }) }}</p>
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
  width: min(980px, 100%);
  height: min(760px, 100%);
  display: flex;
  flex-direction: column;
  padding: 20px 20px 0;
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
  gap: 12px;
  margin-bottom: 12px;
}
.wb-tpl-search {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
  width: min(260px, 40%);
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid var(--color-border, #e6e6e6);
  color: var(--color-text-secondary, #52525b);
}
.dark .wb-tpl-search {
  border-color: var(--color-border-dark, #333);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-tpl-search:focus-within {
  border-color: var(--color-accent);
}
.wb-tpl-search input {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  background: transparent;
  outline: none;
  color: var(--color-text-primary, inherit);
}
.wb-tpl-groups {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-tpl-groups {
  border-color: var(--color-border-dark, #333);
}
.wb-tpl-group {
  padding: 4px 12px;
  border-radius: 999px;
  font-size: 13px;
  border: 1px solid var(--color-border, #e6e6e6);
  color: var(--color-text-secondary, #52525b);
}
.dark .wb-tpl-group {
  border-color: var(--color-border-dark, #333);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-tpl-group:hover {
  border-color: var(--color-accent);
}
.wb-tpl-group.active {
  border-color: var(--color-accent);
  background: color-mix(in srgb, var(--color-accent) 14%, transparent);
  color: var(--color-accent);
  font-weight: 600;
}
.wb-tpl-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 2px 20px;
}
.wb-tpl-section + .wb-tpl-section {
  margin-top: 22px;
}
.wb-tpl-section-title {
  margin-bottom: 10px;
  font-size: 13px;
  font-weight: 700;
}
.wb-tpl-empty {
  padding: 40px 0;
  text-align: center;
  font-size: 13px;
  color: var(--color-text-secondary, #52525b);
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
