<script lang="ts">
import type { LibraryItem as Item } from '../shapeLibraries';
import { boardPreview as drawPreview } from '../../../shared/boardPreview';
import { newBoardData as emptyBoard } from '../boardFile';

/**
 * Where an arrow key moves focus in a grid of `count` buttons laid out
 * `columns` to a row, from `index`. `null` for a key the grid does not use,
 * so Enter and Space still press the button. Stops at the edges rather than
 * wrapping: a grid that wraps from the last icon to the first reads as a jump.
 */
export function gridMove(key: string, index: number, count: number, columns: number): number | null {
  if (count <= 0) return null;
  const cols = Math.max(1, columns);
  const last = count - 1;
  switch (key) {
    case 'ArrowRight': return Math.min(index + 1, last);
    case 'ArrowLeft': return Math.max(index - 1, 0);
    case 'ArrowDown': return index + cols <= last ? index + cols : index;
    case 'ArrowUp': return index - cols >= 0 ? index - cols : index;
    case 'Home': return 0;
    case 'End': return last;
    default: return null;
  }
}

/**
 * How many buttons sit on a grid's first row, from each one's distance from
 * the top. Measured rather than worked out from the CSS, which says
 * `auto-fill` and leaves the answer to the width of the popover.
 */
export function columnsOf(tops: number[]): number {
  if (!tops.length) return 1;
  let n = 0;
  while (n < tops.length && Math.abs(tops[n] - tops[0]) < 1) n++;
  return Math.max(1, n);
}

/**
 * A library piece's preview, drawn once. The libraries tab filters on every
 * keystroke; it used to redraw every piece's SVG each time as well. Keyed by
 * the piece itself, so a library that is re-read gets fresh drawings and the
 * old ones go with the old objects.
 */
const previews = new WeakMap<Item, string>();
export function libraryPreview(item: Item): string {
  let svg = previews.get(item);
  if (svg === undefined) {
    // Escaped by the preview; a piece from a file is drawn, never run.
    svg = item.nodes.length ? drawPreview({ ...emptyBoard(''), nodes: item.nodes, edges: item.edges }) : '';
    previews.set(item, svg);
  }
  return svg;
}
</script>

<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Search, Upload, Trash2 } from 'lucide-vue-next';
import { SHAPES } from '../shapes';
import { loadIcons, findIcons, type IconEntry } from '../iconCatalog';
import type { Glyph } from '../glyph';
import type { LibraryItem, ShapeLibrary } from '../shapeLibraries';
import { fold } from '../boardSearch';

/**
 * Everything that can be put on a board as a piece: the drawn shapes, the
 * icons, and the user's own libraries — with one search across all three.
 *
 * Three hundred shapes and seventeen hundred icons are not browsed, they are
 * looked for, so the search box has focus when the picker opens and a word
 * typed in it narrows every tab at once.
 */
const props = defineProps<{ libraries: ShapeLibrary[] }>();
const emit = defineEmits<{
  (e: 'pick-shape', id: string): void;
  (e: 'pick-icon', glyph: Glyph): void;
  (e: 'pick-item', item: LibraryItem): void;
  (e: 'import-library'): void;
  (e: 'remove-library', path: string): void;
}>();
const { t } = useI18n();

type Tab = 'shapes' | 'icons' | 'libraries';
const tab = ref<Tab>('shapes');
const query = ref('');
const search = ref<HTMLInputElement | null>(null);
onMounted(() => nextTick(() => search.value?.focus()));

// ─── Shapes ─────────────────────────────────────────────────
const categories = computed(() => {
  const keys: string[] = [];
  for (const s of SHAPES) if (!keys.includes(s.category)) keys.push(s.category);
  // Without accents, so "tron" finds "Tròn"; in English too, so "circle"
  // finds it whatever language the app is in.
  const words = fold(query.value.trim());
  const found = (s: (typeof SHAPES)[number]) =>
    !words || fold(t(s.labelKey)).includes(words) || fold(t(s.labelKey, {}, { locale: 'en' })).includes(words) || s.id.toLowerCase().includes(words);
  return keys
    .map((key) => ({
      key,
      label: t(`whiteboard.shape_category.${key}`),
      shapes: SHAPES.filter((s) => s.category === key && found(s)),
    }))
    .filter((c) => c.shapes.length);
});
/**
 * A shape's preview in its own proportions, as it will be put down: a status
 * bar is a strip, a phone is tall. Squeezed into a square, half the wireframe
 * set looked like the same box.
 */
function previewSize(shape: { defaultWidth: number; defaultHeight: number }) {
  const ratio = shape.defaultWidth / shape.defaultHeight || 1;
  const side = 26;
  return ratio >= 1
    ? { width: `${side}px`, height: `${Math.max(6, side / ratio)}px` }
    : { width: `${Math.max(6, side * ratio)}px`, height: `${side}px` };
}
const shapeCount = computed(() => categories.value.reduce((n, c) => n + c.shapes.length, 0));

// ─── Icons ──────────────────────────────────────────────────
const icons = ref<IconEntry[] | null>(null);
const iconsFailed = ref(false);
const shown = ref(120);
async function ensureIcons() {
  if (icons.value || iconsFailed.value) return;
  try {
    icons.value = await loadIcons();
  } catch {
    iconsFailed.value = true;
  }
}
const foundIcons = computed(() => (icons.value ? findIcons(icons.value, query.value) : []));
watch(query, () => { shown.value = 120; });
// Looking for a word with no shape by that name: the icons may have it.
watch([query, tab], () => { if (tab.value === 'icons' || query.value.trim()) void ensureIcons(); });

// ─── Libraries ──────────────────────────────────────────────
const libraryViews = computed(() => {
  const words = query.value.trim().toLowerCase();
  return props.libraries
    .map((lib) => ({
      lib,
      items: lib.items
        .filter((i) => !words || i.title.toLowerCase().includes(words) || lib.name.toLowerCase().includes(words))
        .map((item) => ({ item, svg: libraryPreview(item) })),
    }))
    .filter((v) => v.items.length || !words);
});
const itemCount = computed(() => libraryViews.value.reduce((n, v) => n + v.items.length, 0));

const tabs = computed<{ key: Tab; label: string; count?: number }[]>(() => [
  { key: 'shapes', label: t('whiteboard.picker.shapes'), count: query.value.trim() ? shapeCount.value : undefined },
  { key: 'icons', label: t('whiteboard.picker.icons'), count: query.value.trim() && icons.value ? foundIcons.value.length : undefined },
  { key: 'libraries', label: t('whiteboard.picker.libraries'), count: query.value.trim() ? itemCount.value : undefined },
]);

/** Arrow keys between the tabs, as a tab list is used. */
function onTabKey(e: KeyboardEvent, i: number) {
  const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
  if (!step) return;
  e.preventDefault();
  const next = tabs.value[(i + step + tabs.value.length) % tabs.value.length];
  tab.value = next.key;
  nextTick(() => document.getElementById(`wb-picker-tab-${next.key}`)?.focus());
}

// ─── Moving around a grid ───────────────────────────────────
// Each grid is one Tab stop, and the arrow keys move inside it — the way a
// grid of choices is used. Before, every one of some 350 buttons was a Tab
// stop of its own, so reaching the libraries meant pressing Tab 350 times.

/** The button in each grid that Tab lands on, by grid; the first if unset. */
const stops = ref<Record<string, string>>({});
const visibleIcons = computed(() => foundIcons.value.slice(0, shown.value));
const gridStops = computed(() => {
  const out: Record<string, string> = {};
  const pick = (grid: string, keys: string[]) => {
    if (!keys.length) return;
    const was = stops.value[grid];
    out[grid] = was !== undefined && keys.includes(was) ? was : keys[0];
  };
  for (const c of categories.value) pick(`shapes:${c.key}`, c.shapes.map((s) => s.id));
  pick('icons', visibleIcons.value.map((i) => i.name));
  for (const v of libraryViews.value) pick(`lib:${v.lib.path}`, v.items.map((e) => e.item.id));
  return out;
});
const tabStop = (grid: string, key: string) => (gridStops.value[grid] === key ? 0 : -1);

function gridButtons(box: HTMLElement): HTMLElement[] {
  return [...box.children].filter((el): el is HTMLElement => el instanceof HTMLElement && el.tagName === 'BUTTON');
}
function onGridFocus(e: FocusEvent, grid: string) {
  const key = (e.target as HTMLElement | null)?.closest<HTMLElement>('[data-key]')?.dataset.key;
  if (key !== undefined) stops.value = { ...stops.value, [grid]: key };
}
function onGridKey(e: KeyboardEvent, grid: string) {
  const box = e.currentTarget as HTMLElement;
  const buttons = gridButtons(box);
  const from = buttons.indexOf((e.target as HTMLElement).closest('button') as HTMLElement);
  if (from < 0) return;
  const next = gridMove(e.key, from, buttons.length, columnsOf(buttons.map((b) => b.offsetTop)));
  if (next === null) return;
  e.preventDefault();
  const to = buttons[next];
  if (to.dataset.key !== undefined) stops.value = { ...stops.value, [grid]: to.dataset.key };
  to.focus();
}

/** Enter in the search box takes the first thing found. */
function takeFirst() {
  if (tab.value === 'shapes' && categories.value[0]) emit('pick-shape', categories.value[0].shapes[0].id);
  else if (tab.value === 'icons' && foundIcons.value[0]) emit('pick-icon', foundIcons.value[0].glyph);
  else if (tab.value === 'libraries') {
    const first = libraryViews.value.find((v) => v.items.length)?.items[0];
    if (first) emit('pick-item', first.item);
  }
}
</script>

<template>
  <div class="wb-picker">
    <label class="wb-picker-search">
      <Search :size="14" aria-hidden="true" />
      <input
        ref="search"
        v-model="query"
        type="search"
        :placeholder="t('whiteboard.picker.search')"
        :aria-label="t('whiteboard.picker.search')"
        @keydown.enter.prevent="takeFirst"
      />
    </label>
    <div class="wb-picker-tabs" role="tablist">
      <button
        v-for="(tb, i) in tabs"
        :id="`wb-picker-tab-${tb.key}`"
        :key="tb.key"
        role="tab"
        type="button"
        class="wb-picker-tab"
        :class="{ 'wb-picker-tab--on': tab === tb.key }"
        :aria-selected="tab === tb.key"
        :tabindex="tab === tb.key ? 0 : -1"
        aria-controls="wb-picker-panel"
        @click="tab = tb.key"
        @keydown="onTabKey($event, i)"
      >
        {{ tb.label }}<span v-if="tb.count !== undefined" class="wb-picker-count">{{ tb.count }}</span>
      </button>
    </div>

    <div id="wb-picker-panel" class="wb-picker-panel" role="tabpanel" :aria-labelledby="`wb-picker-tab-${tab}`">
      <!-- Shapes -->
      <template v-if="tab === 'shapes'">
        <p v-if="!categories.length" class="wb-picker-empty">{{ t('whiteboard.picker.none') }}</p>
        <section v-for="cat in categories" :key="cat.key" class="wb-picker-group">
          <h3 class="wb-picker-label">{{ cat.label }}</h3>
          <div class="wb-picker-grid" role="group" :aria-label="cat.label" @keydown="onGridKey($event, `shapes:${cat.key}`)" @focusin="onGridFocus($event, `shapes:${cat.key}`)">
            <button
              v-for="shape in cat.shapes"
              :key="shape.id"
              type="button"
              class="wb-picker-btn"
              :data-key="shape.id"
              :tabindex="tabStop(`shapes:${cat.key}`, shape.id)"
              :title="t(shape.labelKey)"
              :aria-label="t(shape.labelKey)"
              @click.stop="emit('pick-shape', shape.id)"
            >
              <svg viewBox="0 0 100 100" preserveAspectRatio="none" :style="previewSize(shape)" aria-hidden="true">
                <path :d="shape.path" fill="currentColor" fill-opacity="0.08" stroke="currentColor" stroke-width="2" stroke-linejoin="round" vector-effect="non-scaling-stroke" fill-rule="evenodd" />
                <path v-for="(deco, di) in (shape.deco || [])" :key="di" :d="deco" fill="none" stroke="currentColor" stroke-width="2" vector-effect="non-scaling-stroke" stroke-linejoin="round" />
              </svg>
            </button>
          </div>
        </section>
      </template>

      <!-- Icons -->
      <template v-else-if="tab === 'icons'">
        <p v-if="iconsFailed" class="wb-picker-empty">{{ t('whiteboard.picker.icons_failed') }}</p>
        <p v-else-if="!icons" class="wb-picker-empty">{{ t('whiteboard.picker.loading') }}</p>
        <p v-else-if="!foundIcons.length" class="wb-picker-empty">{{ t('whiteboard.picker.none') }}</p>
        <div v-else class="wb-picker-grid" role="group" :aria-label="t('whiteboard.picker.icons')" @keydown="onGridKey($event, 'icons')" @focusin="onGridFocus($event, 'icons')">
          <button
            v-for="icon in visibleIcons"
            :key="icon.name"
            type="button"
            class="wb-picker-btn"
            :data-key="icon.name"
            :tabindex="tabStop('icons', icon.name)"
            :title="icon.words"
            :aria-label="icon.words"
            @click.stop="emit('pick-icon', icon.glyph)"
          >
            <svg viewBox="0 0 24 24" class="wb-picker-ico" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <component :is="part[0]" v-for="(part, pi) in icon.glyph.parts" :key="pi" v-bind="part[1]" />
            </svg>
          </button>
        </div>
        <button v-if="icons && foundIcons.length > shown" type="button" class="wb-picker-more" @click="shown += 240">
          {{ t('whiteboard.picker.more', { count: foundIcons.length - shown }) }}
        </button>
        <p class="wb-picker-note">{{ t('whiteboard.picker.icons_credit') }}</p>
      </template>

      <!-- Libraries -->
      <template v-else>
        <p v-if="!libraries.length" class="wb-picker-empty">{{ t('whiteboard.picker.no_libraries') }}</p>
        <p v-else-if="!libraryViews.length" class="wb-picker-empty">{{ t('whiteboard.picker.none') }}</p>
        <section v-for="v in libraryViews" :key="v.lib.path" class="wb-picker-group">
          <div class="wb-picker-libhead">
            <h3 class="wb-picker-label">{{ v.lib.name }}</h3>
            <button type="button" class="wb-picker-x" :title="t('whiteboard.picker.remove_library', { name: v.lib.name })" :aria-label="t('whiteboard.picker.remove_library', { name: v.lib.name })" @click.stop="emit('remove-library', v.lib.path)">
              <Trash2 :size="13" />
            </button>
          </div>
          <div class="wb-picker-items" role="group" :aria-label="v.lib.name" @keydown="onGridKey($event, `lib:${v.lib.path}`)" @focusin="onGridFocus($event, `lib:${v.lib.path}`)">
            <button
              v-for="entry in v.items"
              :key="entry.item.id"
              type="button"
              class="wb-picker-item"
              :data-key="entry.item.id"
              :tabindex="tabStop(`lib:${v.lib.path}`, entry.item.id)"
              :title="entry.item.title"
              :aria-label="entry.item.title"
              @click.stop="emit('pick-item', entry.item)"
            >
              <img v-if="entry.item.image" :src="entry.item.image.dataUri" alt="" class="wb-picker-thumb" />
              <!-- eslint-disable-next-line vue/no-v-html -->
              <span v-else class="wb-picker-thumb" v-html="entry.svg" />
              <span class="wb-picker-item-title">{{ entry.item.title }}</span>
            </button>
          </div>
        </section>
        <button type="button" class="wb-picker-import" @click.stop="emit('import-library')">
          <Upload :size="14" aria-hidden="true" /> {{ t('whiteboard.picker.import') }}
        </button>
        <p class="wb-picker-note">{{ t('whiteboard.picker.import_hint') }}</p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.wb-picker { display: flex; flex-direction: column; gap: 8px; min-height: 0; max-height: inherit; }
.wb-picker-search { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-radius: 8px; border: 1px solid var(--color-border, #e6e6e6); color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-picker-search) { border-color: var(--color-border-dark, #333); color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-picker-search input { flex: 1; min-width: 0; border: none; outline: none; background: transparent; color: var(--color-text, #18181b); font-size: 13px; }
:global(.dark .wb-picker-search input) { color: var(--color-text-dark, #fafafa); }
.wb-picker-search:focus-within { border-color: var(--color-accent); }
.wb-picker-tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--color-border, #e6e6e6); }
:global(.dark .wb-picker-tabs) { border-color: var(--color-border-dark, #2c2c2c); }
.wb-picker-tab { flex: 1; padding: 6px 4px; font-size: 12px; font-weight: 600; color: var(--color-text-secondary, #52525b); border-bottom: 2px solid transparent; margin-bottom: -1px; display: inline-flex; justify-content: center; gap: 4px; }
:global(.dark .wb-picker-tab) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-picker-tab--on { color: var(--color-accent); border-bottom-color: var(--color-accent); }
:global(.dark .wb-picker-tab--on) { color: var(--color-accent-dark, var(--color-accent)); border-bottom-color: currentColor; }
.wb-picker-count { font-weight: 500; opacity: 0.75; font-variant-numeric: tabular-nums; }
.wb-picker-panel { overflow-y: auto; min-height: 120px; padding-right: 2px; }
.wb-picker-group + .wb-picker-group { margin-top: 8px; padding-top: 8px; border-top: 1px solid var(--color-border, #e6e6e6); }
:global(.dark .wb-picker-group + .wb-picker-group) { border-color: var(--color-border-dark, #2c2c2c); }
.wb-picker-label { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-text-secondary, #52525b); margin-bottom: 6px; padding-left: 2px; }
:global(.dark .wb-picker-label) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-picker-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(38px, 1fr)); gap: 4px; }
.wb-picker-btn { height: 38px; display: grid; place-items: center; border-radius: 8px; color: var(--color-text-secondary, #52525b); }
:global(.dark .wb-picker-btn) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-picker-btn:hover, .wb-picker-btn:focus-visible { background: var(--color-accent); color: #fff; }
.wb-picker-ico { width: 24px; height: 24px; }
.wb-picker-empty, .wb-picker-note { font-size: 12px; color: var(--color-text-secondary, #52525b); padding: 6px 2px; }
:global(.dark .wb-picker-empty), :global(.dark .wb-picker-note) { color: var(--color-text-secondary-dark, #a1a1aa); }
.wb-picker-note { font-size: 12px; }
.wb-picker-more, .wb-picker-import { margin-top: 8px; width: 100%; display: inline-flex; justify-content: center; align-items: center; gap: 6px; padding: 6px; border-radius: 8px; font-size: 12px; font-weight: 600; border: 1px solid var(--color-border, #e6e6e6); }
:global(.dark .wb-picker-more), :global(.dark .wb-picker-import) { border-color: var(--color-border-dark, #333); }
.wb-picker-libhead { display: flex; align-items: center; justify-content: space-between; }
.wb-picker-x { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; color: var(--color-text-secondary, #52525b); margin-bottom: 6px; }
.wb-picker-x:hover { background: rgb(239 68 68 / 0.12); color: #dc2626; }
.wb-picker-items { display: grid; grid-template-columns: repeat(auto-fill, minmax(92px, 1fr)); gap: 6px; }
.wb-picker-item { display: flex; flex-direction: column; align-items: stretch; gap: 4px; padding: 4px; border-radius: 8px; border: 1px solid var(--color-border, #e6e6e6); text-align: center; }
:global(.dark .wb-picker-item) { border-color: var(--color-border-dark, #333); }
.wb-picker-item:hover, .wb-picker-item:focus-visible { border-color: var(--color-accent); }
.wb-picker-thumb { display: block; height: 56px; width: 100%; object-fit: contain; }
.wb-picker-thumb :deep(svg) { width: 100%; height: 100%; }
.wb-picker-item-title { font-size: 12px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; color: var(--color-text, #18181b); }
:global(.dark .wb-picker-item-title) { color: var(--color-text-dark, #fafafa); }
</style>
