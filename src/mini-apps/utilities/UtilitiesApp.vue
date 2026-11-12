<script setup lang="ts">
/**
 * Utilities — small tools for files, run on this machine.
 *
 * The home screen is a grid of tools by group; the sidebar holds the pinned
 * ones, the recent ones, and results not yet saved. Opening a tool is a route
 * query (`?tool=merge`), so the back button goes from a tool to the grid the
 * way it goes between screens everywhere else.
 *
 * Files can come first: dropped anywhere on the home screen, or chosen with
 * "Choose files…", they open "What to do with these?" (`SmartDrop`). Dropped
 * on an open tool, they join it.
 *
 * Design: `docs/utilities-2026-11-11.md` §5; the engine is
 * `src-tauri/src/utilities`.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Info, LayoutGrid, Search, Star, Upload, Wrench, X } from 'lucide-vue-next';
import AppHeader from '../../shared/components/AppHeader.vue';
import ToolView from './components/ToolView.vue';
import SmartDrop from './components/SmartDrop.vue';
import { useWorkbench } from './useWorkbench';
import * as api from './api';
import { CATEGORIES, TOOLS, availability, matches, sortDrop, toolById, type CategoryId, type InputKind, type ToolId } from './tools';
import { KEEP_HOURS } from './constants';
import { showAppNotice } from '../../composables/useAppNotice';
import { logger } from '../../utils/logger';
import { errorText } from '../../shared/errorText';

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const wb = useWorkbench();

// ── Pinned and recent, kept on this device ─────────────────────────────────
const PINS_KEY = 'synabit.utilities.pinned';
const RECENT_KEY = 'synabit.utilities.recent';

function recallIds(key: string, fallback: ToolId[]): ToolId[] {
  try {
    const raw = JSON.parse(localStorage.getItem(key) ?? 'null');
    if (Array.isArray(raw)) return raw.filter((id): id is ToolId => !!toolById(id));
  } catch {
    /* fall through */
  }
  return fallback;
}

const pinned = ref<ToolId[]>(recallIds(PINS_KEY, ['merge', 'split', 'img2pdf']));
const recent = ref<ToolId[]>(recallIds(RECENT_KEY, []));
watch(pinned, (v) => localStorage.setItem(PINS_KEY, JSON.stringify(v)), { deep: true });
watch(recent, (v) => localStorage.setItem(RECENT_KEY, JSON.stringify(v)), { deep: true });

function togglePin(id: ToolId) {
  pinned.value = pinned.value.includes(id) ? pinned.value.filter((p) => p !== id) : [...pinned.value, id];
}

const recentShown = computed(() => recent.value.filter((id) => !pinned.value.includes(id)).slice(0, 3));

// ── Navigation ─────────────────────────────────────────────────────────────
const query = ref('');
const searchBox = ref<HTMLInputElement | null>(null);
const category = ref<CategoryId | 'all'>('all');

function goTool(id: ToolId) {
  router.push({ query: { tool: id } });
}

function goHome(c?: CategoryId | 'all') {
  if (c) category.value = c;
  if (route.query.tool) router.push({ query: {} });
}

function remember(id: ToolId) {
  recent.value = [id, ...recent.value.filter((r) => r !== id)].slice(0, 6);
}

/**
 * The route says which tool is open; this keeps the workbench in step. The
 * app stays alive while another is shown, so this hears every route: only
 * its own count. Coming back with no tool in the route (the rail button) is
 * coming back to where the person was, not to the home grid.
 */
watch(
  () => [route.name, route.query.tool] as const,
  ([name, id], old) => {
    if (name !== 'utilities') return;
    if (old && old[0] !== 'utilities' && !id && wb.tool.value) {
      void router.replace({ query: { tool: wb.tool.value } });
      return;
    }
    const tool = toolById(typeof id === 'string' ? id : null);
    if (!tool) {
      if (wb.tool.value) wb.closeTool();
      return;
    }
    if (wb.tool.value !== tool.id) wb.openTool(tool.id);
    remember(tool.id);
    query.value = '';
  },
  { immediate: true },
);

const tool = computed(() => wb.current.value);

// ── Home grid ──────────────────────────────────────────────────────────────
const groups = computed(() =>
  CATEGORIES.filter((c) => category.value === 'all' || category.value === c)
    .map((c) => ({
      id: c,
      tools: TOOLS.filter(
        (tl) => tl.category === c && matches(query.value, tl, t(`utilities.tools.${tl.id}.name`), t(`utilities.tools.${tl.id}.desc`)),
      ),
    }))
    .filter((g) => g.tools.length),
);

function onSearch(value: string) {
  query.value = value;
  if (wb.tool.value) goHome();
}

// ── Getting files in ───────────────────────────────────────────────────────
const drop = ref<{ kind: InputKind | null; files: api.Input[]; leftOut: number } | null>(null);

async function offer(inputs: api.Input[]) {
  if (!inputs.length) return;
  const sorted = sortDrop(inputs);
  drop.value = { kind: sorted.kind, files: sorted.taken, leftOut: sorted.left.length };
}

function chooseFromDrop(id: ToolId) {
  const files = drop.value?.files ?? [];
  drop.value = null;
  wb.openTool(id, files);
  goTool(id);
}

async function addToTool(inputs: api.Input[]) {
  const refused = wb.addInputs(inputs);
  if (refused) {
    showAppNotice(t('utilities.drop.refused', { n: refused, what: t(`utilities.accepts.${tool.value!.accepts}`) }, refused));
  }
}

async function pick() {
  try {
    if (tool.value) await addToTool(await api.pickInputs([tool.value.accepts], tool.value.max !== 1));
    else await offer(await api.pickInputs(['pdf', 'image', 'office'], true));
  } catch (e) {
    logger.error('[utilities] picking files failed', e);
    showAppNotice(t('utilities.errors.other', { reason: errorText(e) }), 'error');
  }
}

const dropping = ref(false);
let depth = 0;
const carriesFiles = (e: DragEvent) => {
  const types = Array.from(e.dataTransfer?.types ?? []);
  return types.includes('Files') && !types.includes('application/x-synabit-utility-card');
};

const busy = () => wb.phase.value === 'running';

function onDragEnter(e: DragEvent) {
  if (!carriesFiles(e) || busy()) return;
  depth++;
  dropping.value = true;
}
function onDragLeave(e: DragEvent) {
  if (!carriesFiles(e)) return;
  depth = Math.max(0, depth - 1);
  if (!depth) dropping.value = false;
}
function onDragOver(e: DragEvent) {
  // While a run works, a drop is refused: the WebView shows "not allowed".
  if (carriesFiles(e) && !busy()) e.preventDefault();
}
async function onDrop(e: DragEvent) {
  if (!carriesFiles(e)) return;
  e.preventDefault();
  depth = 0;
  dropping.value = false;
  if (busy()) return;
  const files = Array.from(e.dataTransfer?.files ?? []);
  try {
    const inputs = await Promise.all(files.map(async (f) => api.stage(f.name, new Uint8Array(await f.arrayBuffer()))));
    if (tool.value && wb.phase.value !== 'done') await addToTool(inputs);
    else await offer(inputs);
  } catch (err) {
    logger.error('[utilities] taking dropped files failed', err);
    showAppNotice(t('utilities.errors.other', { reason: errorText(err) }), 'error');
  }
}

// ── Results not yet saved ──────────────────────────────────────────────────
function hoursLeft(job: api.Job) {
  return Math.max(1, Math.ceil((job.createdMs + KEEP_HOURS * 3600_000 - Date.now()) / 3600_000));
}

function openResult(job: api.Job) {
  wb.showJob(job);
  router.push({ query: { tool: job.tool } });
}

async function discardResult(job: api.Job) {
  await api.discard(job.id).catch(logger.error);
  if (wb.job.value?.id === job.id) wb.editAgain();
  void wb.refreshUnsaved();
}

async function chain(id: ToolId) {
  await wb.chain(id);
  goTool(id);
}

function onDiscarded() {
  wb.editAgain();
  void wb.refreshUnsaved();
}

// ── Header ─────────────────────────────────────────────────────────────────
const primaryLabel = computed(() => {
  if (!tool.value) return t('utilities.choose_files');
  if (wb.phase.value === 'done' || wb.phase.value === 'empty') return undefined;
  if (availability(tool.value, wb.vision.value, wb.office.value) !== 'ready') return undefined;
  const n = wb.usable.value.length;
  return t(`utilities.tools.${tool.value.id}.verb`, { n }, n);
});

function onPrimary() {
  if (!tool.value) void pick();
  else void wb.run();
}

function onKey(e: KeyboardEvent) {
  const typing = (e.target as HTMLElement)?.closest('input, textarea, [contenteditable="true"]');
  if (e.key === '/' && !typing) {
    e.preventDefault();
    searchBox.value?.focus();
  } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && wb.ready.value) {
    e.preventDefault();
    void wb.run();
  } else if (e.key === 'Escape' && wb.phase.value === 'running') {
    wb.cancel();
  }
}

onMounted(() => {
  void wb.askVision();
  void wb.askOffice();
  void wb.listenForProgress();
  void wb.refreshUnsaved();
  window.addEventListener('keydown', onKey);
});

// Kept alive while another app is shown: its keys are not this app's.
onActivated(() => window.addEventListener('keydown', onKey));
onDeactivated(() => window.removeEventListener('keydown', onKey));

onBeforeUnmount(() => {
  wb.stopListening();
  window.removeEventListener('keydown', onKey);
});
</script>

<template>
  <div
    class="relative h-full flex min-h-0 bg-base dark:bg-base-dark"
    @dragenter="onDragEnter"
    @dragleave="onDragLeave"
    @dragover="onDragOver"
    @drop="onDrop"
  >
    <!-- Sidebar -->
    <aside class="w-64 shrink-0 hidden md:flex flex-col border-r border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark overflow-y-auto" :aria-label="t('utilities.sidebar.label')">
      <div class="px-3 pt-3 pb-2">
        <label class="flex items-center gap-2 h-[34px] px-2.5 rounded-lg border border-border dark:border-border-dark bg-surface dark:bg-surface-dark text-muted dark:text-muted-dark focus-within:border-accent">
          <Search class="w-4 h-4 shrink-0" aria-hidden="true" />
          <input
            ref="searchBox"
            :value="query"
            type="search"
            class="flex-1 min-w-0 bg-transparent outline-none text-sm text-text dark:text-text-dark"
            :placeholder="t('utilities.sidebar.search')"
            :aria-label="t('utilities.sidebar.search')"
            @input="onSearch(($event.target as HTMLInputElement).value)"
          />
          <kbd class="text-xs border border-border dark:border-border-dark rounded px-1.5">/</kbd>
        </label>
      </div>

      <template v-if="pinned.length">
        <h2 class="utility-section">{{ t('utilities.sidebar.pinned') }}</h2>
        <button v-for="id in pinned" :key="`pin-${id}`" type="button" class="utility-nav" :class="{ on: tool?.id === id }" :aria-current="tool?.id === id ? 'page' : undefined" @click="goTool(id)">
          <component :is="toolById(id)!.icon" class="w-4 h-4 shrink-0" aria-hidden="true" />{{ t(`utilities.tools.${id}.name`) }}
        </button>
      </template>

      <template v-if="recentShown.length">
        <h2 class="utility-section">{{ t('utilities.sidebar.recent') }}</h2>
        <button v-for="id in recentShown" :key="`recent-${id}`" type="button" class="utility-nav" :class="{ on: tool?.id === id }" @click="goTool(id)">
          <component :is="toolById(id)!.icon" class="w-4 h-4 shrink-0" aria-hidden="true" />{{ t(`utilities.tools.${id}.name`) }}
        </button>
      </template>

      <h2 class="utility-section">{{ t('utilities.sidebar.groups') }}</h2>
      <button type="button" class="utility-nav" :class="{ on: !tool && category === 'all' }" @click="goHome('all')">
        <LayoutGrid class="w-4 h-4 shrink-0" aria-hidden="true" />{{ t('utilities.sidebar.all') }}<span class="ml-auto text-xs text-muted">{{ TOOLS.length }}</span>
      </button>
      <button v-for="c in CATEGORIES" :key="c" type="button" class="utility-nav" :class="{ on: !tool && category === c }" @click="goHome(c)">
        <span class="w-2 h-2 rounded-[3px] shrink-0" :class="`utility-dot-${c}`" aria-hidden="true" />{{ t(`utilities.categories.${c}.name`) }}
        <span class="ml-auto text-xs text-muted">{{ TOOLS.filter((x) => x.category === c).length }}</span>
      </button>

      <template v-if="wb.unsaved.value.length">
        <h2 class="utility-section">{{ t('utilities.sidebar.unsaved') }}</h2>
        <div v-for="job in wb.unsaved.value" :key="job.id" class="group relative">
          <button type="button" class="utility-nav !items-start pr-8" :class="{ on: wb.job.value?.id === job.id }" @click="openResult(job)">
            <component :is="toolById(job.tool)?.icon ?? Wrench" class="w-4 h-4 shrink-0 mt-0.5" aria-hidden="true" />
            <span class="min-w-0">
              <span class="block truncate">{{ job.outputs.length === 1 ? job.outputs[0].name : t('utilities.sidebar.n_files', { n: job.outputs.length }, job.outputs.length) }}</span>
              <span class="block text-xs text-muted dark:text-muted-dark truncate">
                {{ toolById(job.tool) ? t(`utilities.tools.${job.tool}.name`) : job.tool }} · {{ t('utilities.sidebar.expires', { hours: hoursLeft(job) }, hoursLeft(job)) }}
              </span>
            </span>
          </button>
          <button type="button" class="absolute right-3 top-1.5 w-6 h-6 rounded-md grid place-items-center text-muted opacity-0 group-hover:opacity-100 focus:opacity-100 hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('utilities.sidebar.discard', { name: job.outputs[0]?.name ?? '' })" @click="discardResult(job)">
            <X class="w-3.5 h-3.5" aria-hidden="true" />
          </button>
        </div>
      </template>

      <p class="mt-auto px-3 py-3 flex gap-2 text-xs leading-snug text-muted dark:text-muted-dark">
        <Info class="w-4 h-4 shrink-0" aria-hidden="true" />{{ t('utilities.sidebar.local') }}
      </p>
    </aside>

    <!-- Main -->
    <div class="flex-1 min-w-0 flex flex-col">
      <AppHeader
        :title="tool ? t(`utilities.tools.${tool.id}.name`) : t('shell.apps.utilities')"
        :subtitle="tool ? t(`utilities.tools.${tool.id}.desc`) : t('utilities.subtitle')"
        :icon="tool ? tool.icon : Wrench"
        :primary-label="primaryLabel"
        :primary-icon="tool ? tool.icon : Upload"
        :primary-disabled="!!tool && !wb.ready.value"
        @primary="onPrimary"
      >
        <!-- Always given, even empty: the slot is what pushes the primary button to the right. -->
        <template #actions>
          <button v-if="tool && wb.phase.value === 'done'" type="button" class="btn-secondary" @click="wb.editAgain()">{{ t('utilities.result.edit_again') }}</button>
          <button v-else-if="tool && wb.phase.value === 'loaded' && (tool.max === undefined || wb.usable.value.length < tool.max)" type="button" class="btn-secondary !border-transparent" @click="pick">{{ t('utilities.files.add') }}</button>
        </template>
      </AppHeader>

      <div class="flex-1 min-h-0 overflow-hidden">
        <ToolView v-if="tool" :key="tool.id" @pick="pick" @chain="chain" @discarded="onDiscarded" />

        <div v-else class="h-full overflow-auto">
          <div class="max-w-[1180px] px-7 pt-5 pb-10">
            <button
              type="button"
              class="w-full flex items-center gap-3.5 rounded-xl border-[1.5px] border-dashed border-border-subtle dark:border-border-subtle-dark bg-surface dark:bg-surface-dark px-4 py-3.5 text-left text-text-secondary dark:text-text-secondary-dark hover:border-accent"
              @click="pick"
            >
              <Upload class="w-5 h-5 shrink-0" aria-hidden="true" />
              <span>
                <span class="block font-semibold text-text dark:text-text-dark">{{ t('utilities.home.drop_title') }}</span>
                <span class="block text-sm">{{ t('utilities.home.drop_body') }}</span>
              </span>
            </button>

            <section v-for="g in groups" :key="g.id" class="mt-6">
              <h2 class="flex items-center gap-2 text-[13px] font-semibold text-text dark:text-text-dark mb-2.5">
                <span class="w-2 h-2 rounded-[3px]" :class="`utility-dot-${g.id}`" aria-hidden="true" />
                {{ t(`utilities.categories.${g.id}.name`) }}
                <span class="font-normal text-muted dark:text-muted-dark">{{ t(`utilities.categories.${g.id}.note`) }}</span>
              </h2>
              <ul class="grid gap-2.5 list-none p-0 m-0" style="grid-template-columns: repeat(auto-fill, minmax(232px, 1fr))">
                <li v-for="tl in g.tools" :key="tl.id" class="group relative">
                  <button
                    type="button"
                    class="w-full h-full flex gap-3 items-start text-left p-3 rounded-xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark hover:border-border-subtle dark:hover:border-border-subtle-dark hover:shadow-sm"
                    :class="['not-mac', 'old-mac'].includes(availability(tl, wb.vision.value, wb.office.value)) ? 'opacity-55' : ''"
                    @click="goTool(tl.id)"
                  >
                    <span class="w-9 h-9 rounded-[9px] grid place-items-center shrink-0" :class="`utility-tint-${tl.category}`">
                      <component :is="tl.icon" class="w-5 h-5" aria-hidden="true" />
                    </span>
                    <span class="min-w-0 pr-5">
                      <span class="flex items-center gap-1.5 font-semibold text-sm text-text dark:text-text-dark">
                        {{ t(`utilities.tools.${tl.id}.name`) }}
                        <span v-if="availability(tl, wb.vision.value, wb.office.value) !== 'ready'" class="text-xs font-semibold px-1.5 rounded-full bg-surface-hover dark:bg-surface-hover-dark text-muted dark:text-muted-dark whitespace-nowrap">
                          {{ t({ 'not-mac': 'utilities.vision.tag_mac', 'old-mac': 'utilities.vision.tag_mac14', 'needs-install': 'utilities.setup.tag', ready: '' }[availability(tl, wb.vision.value, wb.office.value)]) }}
                        </span>
                      </span>
                      <span class="block text-[12.5px] leading-snug mt-0.5 text-text-secondary dark:text-text-secondary-dark">{{ t(`utilities.tools.${tl.id}.desc`) }}</span>
                    </span>
                  </button>
                  <button
                    type="button"
                    class="absolute top-2 right-2 w-7 h-7 rounded-md grid place-items-center focus:opacity-100"
                    :class="pinned.includes(tl.id) ? 'text-yellow-500 opacity-100' : 'text-muted opacity-0 group-hover:opacity-100'"
                    :aria-pressed="pinned.includes(tl.id)"
                    :aria-label="t(pinned.includes(tl.id) ? 'utilities.home.unpin' : 'utilities.home.pin', { name: t(`utilities.tools.${tl.id}.name`) })"
                    :title="t(pinned.includes(tl.id) ? 'utilities.home.unpin' : 'utilities.home.pin', { name: t(`utilities.tools.${tl.id}.name`) })"
                    @click="togglePin(tl.id)"
                  >
                    <Star class="w-4 h-4" :fill="pinned.includes(tl.id) ? 'currentColor' : 'none'" aria-hidden="true" />
                  </button>
                </li>
              </ul>
            </section>

            <p v-if="!groups.length" class="py-10 text-center text-text-secondary dark:text-text-secondary-dark">
              {{ t('utilities.home.no_match', { query }) }}
            </p>
          </div>
        </div>
      </div>
    </div>

    <div
      v-if="dropping"
      class="pointer-events-none absolute inset-2.5 z-40 rounded-2xl border-[2.5px] border-dashed border-accent bg-accent/5 grid place-items-center text-lg font-semibold text-accent dark:text-accent-dark"
    >
      {{ tool && wb.phase.value !== 'done' ? t('utilities.drop.add_here', { tool: t(`utilities.tools.${tool.id}.name`) }) : t('utilities.drop.see_what') }}
    </div>

    <SmartDrop
      :show="!!drop"
      :kind="drop?.kind ?? null"
      :files="drop?.files ?? []"
      :left-out="drop?.leftOut ?? 0"
      @close="drop = null"
      @choose="chooseFromDrop"
    />
  </div>
</template>

<style>
/* Shared by the Utilities components. Not scoped, so the children use them;
   prefixed so nothing else in the app can collide with them. */
.utility-tint-pdf { background: color-mix(in oklab, #d93838 13%, transparent); color: #c42b2b; }
.utility-tint-convert { background: color-mix(in oklab, var(--color-accent) 13%, transparent); color: var(--color-accent); }
.dark .utility-tint-pdf { color: #f87171; }
.dark .utility-tint-convert { color: var(--color-accent-dark); }
.utility-dot-pdf { background: #d93838; }
.utility-dot-convert { background: var(--color-accent); }
.utility-tint-image { background: color-mix(in oklab, #7c3aed 13%, transparent); color: #6d28d9; }
.dark .utility-tint-image { color: #a78bfa; }
.utility-dot-image { background: #7c3aed; }

.utility-section {
  padding: 12px 12px 2px 20px;
  font-size: 12px; font-weight: 600; letter-spacing: .05em; text-transform: uppercase;
  color: var(--color-text-tertiary);
}
.dark .utility-section { color: var(--color-text-tertiary-dark); }
.utility-nav {
  display: flex; align-items: center; gap: 9px; width: calc(100% - 16px); margin: 1px 8px; padding: 6px 8px;
  border-radius: 7px; text-align: left; font-size: 14px; color: var(--color-text-secondary);
}
.dark .utility-nav { color: var(--color-text-secondary-dark); }
.utility-nav:hover { background: var(--color-surface-hover); }
.dark .utility-nav:hover { background: var(--color-surface-hover-dark); }
.utility-nav.on { background: color-mix(in oklab, var(--color-accent) 12%, transparent); color: var(--color-accent); font-weight: 500; }
.dark .utility-nav.on { color: var(--color-accent-dark); }

.utility-label {
  margin: 0 0 10px; font-size: 12px; font-weight: 600; letter-spacing: .05em; text-transform: uppercase;
  color: var(--color-text-tertiary);
}
.dark .utility-label { color: var(--color-text-tertiary-dark); }
.utility-sub { margin: 12px 0 6px; font-size: 12px; color: var(--color-text-tertiary); }
.dark .utility-sub { color: var(--color-text-tertiary-dark); }
.utility-seg { display: flex; gap: 2px; padding: 3px; border-radius: 8px; background: var(--color-surface-hover); }
.dark .utility-seg { background: var(--color-surface-hover-dark); }
.utility-seg button { flex: 1; height: 28px; border-radius: 6px; font-size: 13px; color: var(--color-text-secondary); white-space: nowrap; padding: 0 6px; }
.dark .utility-seg button { color: var(--color-text-secondary-dark); }
.utility-seg button.on { background: var(--color-surface); color: var(--color-text); font-weight: 500; box-shadow: 0 1px 2px rgb(0 0 0 / .08); }
.dark .utility-seg button.on { background: var(--color-surface-dark); color: var(--color-text-dark); }
.utility-field {
  width: 100%; height: 34px; padding: 0 10px; border-radius: 8px; outline: none; font-size: 14px;
  border: 1px solid var(--color-border); background: var(--color-surface); color: var(--color-text);
}
.dark .utility-field { border-color: var(--color-border-dark); background: var(--color-surface-dark); color: var(--color-text-dark); }
.utility-field:focus { border-color: var(--color-accent); }
.utility-field[aria-invalid='true'] { border-color: var(--color-danger); }
</style>
