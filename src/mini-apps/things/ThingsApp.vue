<script setup lang="ts">
/**
 * Things — the app that does not know what it shows.
 *
 * Every other mini-app is built around one type it understands. This one asks
 * the vault what is in it and draws that, so a `type: animal` somebody typed
 * into a file appears here with no registration step, no manifest, and no code
 * change.
 *
 * T1 is read-only on purpose: a list that cannot be edited cannot corrupt
 * anything, and the question this stage answers — does a generic list over an
 * arbitrary type work, and is it fast enough — does not need writes to answer.
 */
import { ref, computed, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Search, RefreshCw, ChevronRight, PanelRight, PanelRightClose, Globe, ArrowUpDown, Rows3, Columns3, List, Table, Plus, Bookmark, Pin, PinOff, Trash2 } from 'lucide-vue-next';
import { useObservedTypes } from './composables/useObservedTypes';
import { useThingsQuery } from './composables/useThingsQuery';
import { useThingsNode } from './composables/useThingsNode';
import { useThingsLinks } from './composables/useThingsLinks';
import { useThingsArrangement } from './composables/useThingsArrangement';
import { useThingsViews, type SavedView } from './composables/useThingsViews';
import { folderForType } from '../../shared/nodeRoutes';
import { useNodeService } from '../../composables/useNodeService';
import { iconForNodeType } from '../../shared/views/nodeTypeIcon';
import ListView from '../../shared/views/ListView.vue';
import TableView from '../../shared/views/TableView.vue';
import ObjectDetail from '../../shared/views/ObjectDetail.vue';
import NoteGraph from '../note/NoteGraph.vue';
import type { QueryRow } from '../../shared/views/types';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();

const { browsable, internal, load: loadTypes, loading: typesLoading, fieldsFor } = useObservedTypes();
const arrange = useThingsArrangement();
const saved = useThingsViews();
const ns = useNodeService();
const { query, result, loading, error, run, showType } = useThingsQuery();
const detail = useThingsNode();
const links = useThingsLinks();

const activeType = ref<string | null>(null);
const showInternal = ref(false);
const selectedId = ref<string | null>(null);
const showRail = ref(true);

/**
 * Which primitive draws the result.
 *
 * The list lives in the left column beside the open node; the table wants the
 * width, so it takes the middle and the node opens over it. Same data, and the
 * views are handed exactly the same `QueryResult` — the layout is the only
 * thing that changes.
 */
const layout = ref<'list' | 'table'>('list');

/**
 * Fields this type's nodes carry, as things to arrange by.
 *
 * Straight from the vault. Nothing here is declared, so `energy` appears in
 * these menus because a file has it, not because anyone added it to a list.
 */
const fields = computed(() => (activeType.value ? fieldsFor(activeType.value) : []));
const sortable = computed(() => arrange.sortableFrom(fields.value));
const groupable = computed(() => arrange.arrangeableFrom(fields.value).filter(f => f !== 'title'));

/** The typed filter and the arrangement, as one string for the engine. */
const rerun = () => run(arrange.compose(typed.value));

/**
 * What the user typed, kept apart from what is sent.
 *
 * The box holds filters; the menus add `sort:` and `columns:` when composing.
 * Keeping them separate means a menu change does not rewrite the text in front
 * of someone mid-sentence.
 */
const typed = ref('');

const openType = (nodeType: string) => {
  activeType.value = nodeType;
  selectedId.value = null;
  detail.close();
  links.clear();
  typed.value = `type:${nodeType}`;
  arrange.reset();
  arrange.suggestColumns(fieldsFor(nodeType));
  rerun();
};

/**
 * Typing a query by hand takes over from the rail.
 *
 * The rail's selection is cleared rather than kept, because a highlighted type
 * beside results that are not of that type is a lie the screen tells for free.
 */
const runTyped = () => {
  activeType.value = null;
  rerun();
};

const refresh = async () => {
  await loadTypes();
  if (typed.value.trim()) await rerun();
};

/**
 * Open a row here rather than routing it somewhere.
 *
 * Sending a `book` to another app would need a list in the code saying which
 * app owns which type — the thing Things exists to do without. Every kind of
 * node opens in the same pane.
 */
const openRow = async (row: QueryRow) => {
  selectedId.value = row.id;
  await Promise.all([detail.open(row.id), links.load(row.id, row.title)]);
};

/** A backlink is a node like any other, so it opens the same way. */
const openLinked = async (id: string, title: string) => {
  selectedId.value = id;
  await Promise.all([detail.open(id), links.load(id, title)]);
};

const total = computed(() => result.value?.total ?? 0);

/**
 * What the graph is given.
 *
 * `NoteGraph` asks for `allNotes` only to look a title up by id — it draws the
 * neighbourhood, not the vault — so the rows currently listed are enough, and
 * cheaper than fetching everything to render a panel.
 */
const graphNeighbours = computed(() =>
  (result.value?.rows ?? []).map(r => ({ id: r.id, title: r.title })),
);

const openTags = computed<string[]>(() => {
  const tags = detail.node.value?.properties?.tags;
  return Array.isArray(tags) ? tags.map(String) : [];
});

/**
 * The graph's inputs, computed rather than built in the template.
 *
 * `NoteGraph` watches its props `deep`, so an array literal written inline —
 * `:outgoing-links="[]"`, or a `.map()` over the backlinks — is a new object on
 * every render of this component, and every keystroke in the query box would
 * wake the watcher and walk the arrays.
 *
 * It stops there rather than redrawing: the graph fingerprints its inputs and
 * debounces by 150ms, so nothing was being re-simulated. The waste was the
 * traversal and a `JSON.stringify` per keystroke, which is small and entirely
 * avoidable by handing it the same array twice.
 */
const graphBacklinks = computed(() =>
  links.backlinks.value.map(b => ({ id: b.id, title: b.title })),
);

/**
 * Nothing, stably.
 *
 * `get_linked_nodes` answers "what points at this", so Things has the incoming
 * half of the graph and not the outgoing half. Passing a fresh `[]` would be a
 * new object each render; passing this one is the same object forever.
 */
// Not `readonly`: NoteGraph declares the prop as `string[]`, and it never
// writes to it.
const NO_OUTGOING: string[] = [];

/**
 * Create a node of whatever type is being browsed.
 *
 * No step that defines the type first: writing the file is what makes the type
 * exist, and it existed already if the rail is showing it. The folder comes
 * from `folderForType`, which the assistant's own writer uses too.
 */
const creating = ref(false);
const newType = ref('');

const startCreate = () => {
  newType.value = activeType.value ?? '';
  creating.value = true;
};

const create = async () => {
  const type = newType.value.trim().toLowerCase();
  if (!type) return;
  creating.value = false;

  const relPath = `${folderForType(type)}/${crypto.randomUUID()}.md`;
  try {
    await ns.writeNode({
      relPath,
      nodeType: type as never,
      title: '',
      properties: {},
      content: '',
      eventType: 'created',
    });
    await loadTypes();
    if (activeType.value !== type) openType(type);
    else await rerun();
    await openRow({ id: relPath, node_type: type, title: '', cells: [] });
  } catch (e) {
    logger.error('[Things] Could not create', e);
  }
};

/** Keep what is on screen, arrangement and all. */
const saveCurrentView = async () => {
  const name = window.prompt(t('things.name_this_view'), activeType.value ?? '');
  if (!name?.trim()) return;
  await saved.save({
    name: name.trim(),
    query: typed.value,
    layout: layout.value,
    sort: arrange.sortField.value,
    sortDescending: arrange.sortDescending.value,
    group: arrange.groupBy.value,
    columns: [...arrange.columns.value],
    home: 'things',
  });
};

const openView = (view: SavedView) => {
  activeType.value = null;
  selectedId.value = null;
  detail.close();
  links.clear();
  typed.value = view.query;
  layout.value = view.layout;
  arrange.sortField.value = view.sort;
  arrange.sortDescending.value = view.sortDescending;
  arrange.groupBy.value = view.group;
  arrange.columns.value = [...view.columns];
  rerun();
};

onMounted(async () => {
  await Promise.all([loadTypes(), saved.load()]);
});
</script>

<template>
  <div class="h-full flex min-h-0 bg-white dark:bg-[#141416]">

    <!-- ── Left: what the vault holds, then what is in it ───── -->
    <aside
      class="w-[260px] flex-shrink-0 border-r border-gray-100 dark:border-[#232326]
             flex flex-col min-h-0 bg-gray-50/60 dark:bg-[#101012]"
    >
      <!--
        Types above the list rather than in a column of their own. A fourth
        column would leave the list about 240px on a 1280px screen, and the
        type list is five to ten entries on a real vault — it does not earn
        the width.
      -->
      <div class="px-4 pt-4 pb-2 flex items-center justify-between flex-shrink-0">
        <h2 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">
          {{ t('things.in_your_vault') }}
        </h2>
        <span class="flex items-center gap-0.5">
          <button
            type="button"
            @click="startCreate"
            class="p-1 rounded text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
            :title="t('things.create')"
          >
            <Plus class="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            @click="refresh"
            class="p-1 rounded text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
            :title="t('things.refresh')"
          >
            <RefreshCw class="w-3.5 h-3.5" :class="typesLoading ? 'animate-spin' : ''" />
          </button>
        </span>
      </div>

      <!--
        Creating something is picking a word. A type nobody has used yet is
        typed in, and it exists the moment the file lands — there is no step
        that declares it first.
      -->
      <div v-if="creating" class="px-3 pb-2 flex-shrink-0">
        <input
          v-model="newType"
          type="text"
          spellcheck="false"
          autofocus
          @keydown.enter="create"
          @keydown.esc="creating = false"
          @blur="creating = false"
          :placeholder="t('things.what_kind')"
          list="things-known-types"
          class="w-full px-2 py-1.5 rounded-lg bg-white dark:bg-white/5
                 border border-violet-300 dark:border-violet-500/40 text-xs
                 text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 outline-none"
        />
        <datalist id="things-known-types">
          <option v-for="entry in browsable" :key="entry.node_type" :value="entry.node_type" />
        </datalist>
      </div>

      <div class="max-h-[38%] overflow-y-auto px-2 pb-2 flex-shrink-0">
        <button
          v-for="entry in browsable"
          :key="entry.node_type"
          type="button"
          @click="openType(entry.node_type)"
          class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm transition-colors cursor-pointer"
          :class="entry.node_type === activeType
            ? 'bg-gray-200/70 dark:bg-white/10 text-[#1c1c1e] dark:text-[#f4f4f5]'
            : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5'"
        >
          <component :is="iconForNodeType(entry.node_type)" class="w-4 h-4 flex-shrink-0 text-gray-400" />
          <!--
            The type's own name, not a translated label. For `note` and `task`
            that reads as English beside a Vietnamese interface; for `animal` it
            is the only name there is. Naming them from a table in the code
            would mean a type nobody coded for has no name at all.
          -->
          <span class="truncate">{{ entry.node_type }}</span>
          <span class="ml-auto text-xs text-gray-400 dark:text-gray-600 tabular-nums">{{ entry.count }}</span>
        </button>

        <p
          v-if="!typesLoading && browsable.length === 0"
          class="px-2 py-3 text-xs text-gray-400 dark:text-gray-500"
        >
          {{ t('things.vault_empty') }}
        </p>

        <!--
          Real, and noise at the top of a list of what you keep. `json` alone
          outnumbers notes on an ordinary vault — feed state, message days,
          whiteboard payloads — so it is here rather than hidden, and folded.
        -->
        <template v-if="internal.length">
          <button
            type="button"
            @click="showInternal = !showInternal"
            class="w-full flex items-center gap-1.5 px-2 py-1.5 mt-2 text-[11px] font-semibold uppercase
                   tracking-wider text-gray-400 dark:text-gray-500 hover:text-gray-600 dark:hover:text-gray-300
                   transition-colors cursor-pointer"
          >
            <ChevronRight class="w-3 h-3 transition-transform" :class="showInternal ? 'rotate-90' : ''" />
            {{ t('things.internal') }}
          </button>
          <button
            v-for="entry in (showInternal ? internal : [])"
            :key="entry.node_type"
            type="button"
            @click="openType(entry.node_type)"
            class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm text-gray-500 dark:text-gray-500
                   hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
          >
            <component :is="iconForNodeType(entry.node_type)" class="w-4 h-4 flex-shrink-0 text-gray-400" />
            <span class="truncate font-mono text-xs">{{ entry.node_type }}</span>
            <span class="ml-auto text-xs text-gray-400 dark:text-gray-600 tabular-nums">{{ entry.count }}</span>
          </button>
        </template>

        <!--
          The ladder, as a list. A view saved here shows up under the types;
          pinning one moves it into the app sidebar beside Notes and Tasks,
          which is a change to one field rather than a different feature.
        -->
        <template v-if="saved.views.value.length">
          <div class="px-2 py-1.5 mt-3 text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">
            {{ t('things.saved_views') }}
          </div>
          <div
            v-for="view in saved.views.value"
            :key="view.id"
            class="group w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-sm
                   text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
          >
            <button type="button" @click="openView(view)" class="flex items-center gap-2.5 min-w-0 flex-1 cursor-pointer">
              <Bookmark class="w-4 h-4 flex-shrink-0" :class="view.home === 'sidebar' ? 'text-violet-500' : 'text-gray-400'" />
              <span class="truncate text-left">{{ view.name }}</span>
            </button>
            <button
              type="button"
              @click="saved.setHome(view, view.home === 'sidebar' ? 'things' : 'sidebar')"
              class="p-0.5 rounded text-gray-300 hover:text-violet-500 opacity-0 group-hover:opacity-100
                     focus:opacity-100 transition-all cursor-pointer"
              :title="view.home === 'sidebar' ? t('things.unpin') : t('things.pin')"
            >
              <PinOff v-if="view.home === 'sidebar'" class="w-3 h-3" />
              <Pin v-else class="w-3 h-3" />
            </button>
            <button
              type="button"
              @click="saved.remove(view)"
              class="p-0.5 rounded text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100
                     focus:opacity-100 transition-all cursor-pointer"
              :title="t('things.delete_view')"
            >
              <Trash2 class="w-3 h-3" />
            </button>
          </div>
        </template>
      </div>

      <div class="px-3 py-2 border-t border-gray-100 dark:border-[#232326] flex-shrink-0">
        <div class="relative">
          <Search class="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-400 pointer-events-none" />
          <input
            v-model="typed"
            type="text"
            spellcheck="false"
            @keydown.enter="runTyped"
            :placeholder="t('things.query_placeholder')"
            class="w-full pl-8 pr-2 py-1.5 rounded-lg bg-white dark:bg-white/5
                   border border-gray-200 dark:border-gray-700/50 text-xs
                   text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 outline-none
                   focus:border-violet-400 dark:focus:border-violet-500/50 transition-colors"
          />
        </div>
        <p v-if="result" class="mt-1.5 px-0.5 text-[11px] text-gray-400 tabular-nums">
          {{ t('things.n_results', { n: total }) }}
        </p>
      </div>

      <!--
        Arrangement. Every option in these three menus comes from the vault:
        `observed_schemas` reports which keys this type's nodes carry, so a
        field somebody typed into one file turns up here without being
        registered anywhere.
      -->
      <div
        v-if="result && fields.length"
        class="px-3 pb-2 flex flex-wrap items-center gap-1.5 flex-shrink-0"
      >
        <label class="relative inline-flex items-center gap-1 text-[11px] text-gray-500 dark:text-gray-400">
          <ArrowUpDown class="w-3 h-3 text-gray-400" />
          <select
            v-model="arrange.sortField.value"
            @change="rerun"
            class="bg-transparent outline-none cursor-pointer max-w-[86px] truncate appearance-none pr-1"
          >
            <option v-for="f in sortable" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>
        <button
          type="button"
          @click="arrange.sortDescending.value = !arrange.sortDescending.value; rerun()"
          class="px-1.5 py-0.5 rounded text-[11px] text-gray-500 dark:text-gray-400
                 hover:bg-gray-200/60 dark:hover:bg-white/5 transition-colors cursor-pointer"
          :title="t('things.sort_direction')"
        >
          {{ arrange.sortDescending.value ? '↓' : '↑' }}
        </button>

        <label class="relative inline-flex items-center gap-1 text-[11px] text-gray-500 dark:text-gray-400">
          <Rows3 class="w-3 h-3 text-gray-400" />
          <select
            v-model="arrange.groupBy.value"
            @change="rerun"
            class="bg-transparent outline-none cursor-pointer max-w-[86px] truncate appearance-none pr-1"
          >
            <option value="">{{ t('things.no_group') }}</option>
            <option v-for="f in groupable" :key="f" :value="f">{{ f }}</option>
          </select>
        </label>

        <button
          type="button"
          @click="saveCurrentView"
          class="ml-auto p-1 rounded text-gray-400 hover:text-violet-500 transition-colors cursor-pointer"
          :title="t('things.save_view')"
        >
          <Bookmark class="w-3.5 h-3.5" />
        </button>

        <span class="inline-flex rounded-md overflow-hidden border border-gray-200 dark:border-gray-700/50">
          <button
            v-for="kind in (['list', 'table'] as const)"
            :key="kind"
            type="button"
            @click="layout = kind"
            class="px-1.5 py-0.5 transition-colors cursor-pointer"
            :class="layout === kind
              ? 'bg-gray-200/70 dark:bg-white/10 text-gray-700 dark:text-gray-200'
              : 'text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5'"
            :title="kind"
          >
            <List v-if="kind === 'list'" class="w-3 h-3" />
            <Table v-else class="w-3 h-3" />
          </button>
        </span>

        <details class="relative">
          <summary
            class="list-none inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[11px]
                   text-gray-500 dark:text-gray-400 hover:bg-gray-200/60 dark:hover:bg-white/5
                   transition-colors cursor-pointer select-none"
          >
            <Columns3 class="w-3 h-3 text-gray-400" />
            {{ arrange.columns.value.length || t('things.columns') }}
          </summary>
          <div
            class="absolute left-0 top-full mt-1 z-30 w-44 max-h-56 overflow-y-auto p-1.5
                   rounded-lg border border-gray-200 dark:border-[#2c2c2c]
                   bg-white dark:bg-[#1a1a1c] shadow-xl"
          >
            <label
              v-for="f in groupable"
              :key="f"
              class="flex items-center gap-2 px-2 py-1 rounded text-xs text-gray-600 dark:text-gray-400
                     hover:bg-gray-100 dark:hover:bg-white/5 cursor-pointer"
            >
              <input
                type="checkbox"
                :checked="arrange.columns.value.includes(f)"
                @change="arrange.toggleColumn(f); rerun()"
                class="accent-violet-500"
              />
              <span class="truncate font-mono">{{ f }}</span>
            </label>
          </div>
        </details>
      </div>

      <!--
        The engine's own words. It says useful things — an unknown sort key, a
        query with nothing to match on — and swallowing them leaves an empty
        list that looks like an empty vault.
      -->
      <p
        v-if="error"
        class="mx-3 mb-2 px-2.5 py-2 rounded-lg bg-red-500/5 border border-red-500/20
               text-[11px] text-red-500 dark:text-red-400 whitespace-pre-wrap break-words flex-shrink-0"
      >
        {{ error }}
      </p>

      <ListView
        v-if="layout === 'list'"
        class="flex-1 min-h-0 border-t border-gray-100 dark:border-[#232326]"
        :result="result"
        :loading="loading"
        :selected-id="selectedId"
        :group-by="arrange.groupBy.value"
        @open="openRow"
      />
      <div v-else class="flex-1 min-h-0 border-t border-gray-100 dark:border-[#232326]"></div>
    </aside>

    <!-- ── Middle: the thing itself ─────────────────────────── -->
    <section class="flex-1 flex flex-col min-w-0 min-h-0 relative">
      <!--
        The table wants the width the list does not. It sits here rather than
        in the left column, and the node opens over it — which is how the Tasks
        board has always behaved, so it is a habit rather than a new rule.
      -->
      <div v-if="layout === 'table' && result" class="flex-1 overflow-auto min-h-0 p-4">
        <TableView
          :result="result"
          :selected-id="selectedId"
          :untitled-label="t('things.untitled')"
          @open="openRow"
        />
      </div>

      <div
        v-else-if="!detail.node.value"
        class="flex-1 flex items-center justify-center px-8 text-center"
      >
        <p class="text-sm text-gray-400 dark:text-gray-500 max-w-sm leading-relaxed">
          {{ t('things.pick_a_type') }}
        </p>
      </div>

      <template v-if="detail.node.value">
        <button
          type="button"
          @click="showRail = !showRail"
          class="absolute top-3 right-3 z-10 p-1.5 rounded-md text-gray-400
                 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
          :title="t('things.toggle_rail')"
        >
          <PanelRightClose v-if="showRail" class="w-4 h-4" />
          <PanelRight v-else class="w-4 h-4" />
        </button>

        <ObjectDetail
          :class="layout === 'table'
            ? 'absolute inset-0 z-20 border-l border-gray-200 dark:border-[#2c2c2c] shadow-2xl'
            : ''"
          v-model:title="detail.title.value"
          v-model:body="detail.body.value"
          v-model:fields="detail.fields.value"
          :node-type="detail.nodeType.value"
          :read-only-rows="detail.readOnlyRows.value"
          :loading="detail.loading.value"
          :saving="detail.saving.value"
          :vault-path="props.vaultPath"
          @save="detail.save"
          @add-field="detail.addField"
          @remove-field="detail.removeField"
        />
      </template>
    </section>

    <!-- ── Right: where this sits in the graph ──────────────── -->
    <aside
      v-if="detail.node.value && showRail"
      class="w-[300px] flex-shrink-0 border-l border-gray-100 dark:border-[#232326]
             flex flex-col min-h-0 bg-[#fbfbfc] dark:bg-[#101012]"
    >
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-gray-100 dark:border-[#232326]">
        <Globe class="w-4 h-4 text-gray-400 mr-2" />
        <span class="font-semibold text-[11px] tracking-wider text-gray-400 dark:text-gray-500 uppercase">
          {{ t('things.graph') }}
        </span>
      </div>

      <!--
        Half the column, matching the Notes sidebar this is lifted from. A
        force simulation in less than that is a hairball rather than a picture.
      -->
      <div class="h-1/2 border-b border-gray-100 dark:border-[#232326] overflow-hidden">
        <NoteGraph
          :current-note-id="detail.node.value.id"
          :current-note-title="detail.title.value || detail.node.value.id"
          :tags="openTags"
          :outgoing-links="NO_OUTGOING"
          :backlinks="graphBacklinks"
          :all-notes="graphNeighbours"
          @open-note="(id: string) => openLinked(id, '')"
        />
      </div>

      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-gray-100 dark:border-[#232326]">
        <span class="font-semibold text-[11px] tracking-wider text-gray-400 dark:text-gray-500 uppercase">
          {{ t('things.linked_mentions') }} ({{ links.backlinks.value.length }})
        </span>
      </div>

      <div class="flex-1 overflow-y-auto p-2 space-y-1 min-h-0">
        <p
          v-if="links.backlinks.value.length === 0"
          class="text-[13px] text-gray-400 text-center py-4"
        >
          {{ t('things.no_linked_mentions') }}
        </p>
        <button
          v-for="bl in links.backlinks.value"
          :key="bl.id"
          type="button"
          @click="openLinked(bl.id, bl.title)"
          class="w-full text-left p-2.5 rounded-lg border border-transparent
                 hover:bg-white dark:hover:bg-[#1e1e20] hover:border-gray-200 dark:hover:border-[#2f2f2f]
                 transition-all cursor-pointer"
        >
          <span class="flex items-center gap-2">
            <component :is="iconForNodeType(bl.node_type)" class="w-3.5 h-3.5 flex-shrink-0 text-gray-400" />
            <span class="truncate text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5]">{{ bl.title || bl.id }}</span>
          </span>
          <span v-if="bl.preview" class="block mt-1 truncate text-[11px] text-gray-400">{{ bl.preview }}</span>
        </button>
      </div>
    </aside>
  </div>
</template>
