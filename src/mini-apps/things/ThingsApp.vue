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
import { ref, computed, onMounted, onBeforeUnmount, nextTick } from 'vue';
import type { Ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Search, RefreshCw, ChevronRight, PanelRight, PanelRightClose, Globe, ArrowUpDown, Rows3, Columns3, List, Table, Plus, Bookmark, Pin, PinOff, Trash2, Monitor, ArrowLeft, ArrowRight, History, Download, Boxes } from 'lucide-vue-next';
import { useObservedTypes, isInternalType } from './composables/useObservedTypes';
import { useSidebarResize } from '../../composables/useSidebarResize';
import { useThingsQuery } from './composables/useThingsQuery';
import { useThingsNode } from './composables/useThingsNode';
import { useThingsLinks } from './composables/useThingsLinks';
import { useThingsArrangement } from './composables/useThingsArrangement';
import { useThingsViews, type SavedView } from './composables/useThingsViews';
import { useThingsSchema, type SchemaField } from './composables/useThingsSchema';
import type { FieldKind } from '../../shared/fieldValue';
import { withProperties } from '../../shared/views/exportMarkdown';
import { isAppOwned, GOVERNED, isAuthoredElsewhere } from '../../shared/fieldRegistry';
import { useNodeService } from '../../composables/useNodeService';
import { iconForNodeType } from '../../shared/views/nodeTypeIcon';
import ListView from '../../shared/views/ListView.vue';
import NodeRowMenu from '../../shared/views/NodeRowMenu.vue';
import TypePicker from '../../shared/views/TypePicker.vue';
import FieldPicker from '../../shared/views/FieldPicker.vue';
import TypeOverview from '../../shared/views/TypeOverview.vue';
import RenameFieldDialog from '../../shared/views/RenameFieldDialog.vue';
import DeleteFieldDialog from '../../shared/views/DeleteFieldDialog.vue';
import RemoveKindDialog from '../../shared/views/RemoveKindDialog.vue';
import RenameKindDialog from '../../shared/views/RenameKindDialog.vue';
import SchemaManager from '../../shared/views/SchemaManager.vue';
import UndoToast from '../../shared/components/UndoToast.vue';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
// Both come from the Notes app rather than being copied. The history modal is
// already node-generic underneath — `list_node_versions` and friends take a
// `relPath`, not a note — and the export modal is a format picker that has
// never cared what kind of thing it is writing out. `ObjectDetail` reaches
// across for `TiptapEditor` on the same reasoning.
import NoteHistoryModal from '../note/NoteHistoryModal.vue';
import NoteExportModal from '../note/NoteExportModal.vue';
import { useNoteExport } from '../note/composables/useNoteExport';
import type { NoteItem } from '../note/helpers';
import { useThingsRowActions, UNDO_WINDOW_SECONDS } from './composables/useThingsRowActions';
import { routeForNodeType } from '../../shared/nodeRoutes';
import { appName } from '../../shared/appRegistry';
import { useRouter } from 'vue-router';
import TableView from '../../shared/views/TableView.vue';
import ObjectDetail from '../../shared/views/ObjectDetail.vue';
import NoteGraph from '../note/NoteGraph.vue';
import type { QueryRow, QueryResult } from '../../shared/views/types';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();

const { types, browsable, internal, load: loadTypes, loading: typesLoading, fieldsFor, observedFor, usualFieldsFor, kindOfField } = useObservedTypes();
const arrange = useThingsArrangement();
const saved = useThingsViews();
const schema = useThingsSchema();
const ns = useNodeService();
const router = useRouter();
const rowActions = useThingsRowActions(() => props.vaultPath);

/** The row whose menu is open, and where on screen to draw it. */
const menuRow = ref<QueryRow | null>(null);
const menuAt = ref<{ x: number; y: number } | null>(null);

const setMenu = (row: QueryRow | null, at: { x: number; y: number } | null) => {
  menuRow.value = row;
  menuAt.value = at;
};

const closeMenu = () => setMenu(null, null);

/**
 * Hand a node to the app that owns its type.
 *
 * `routeForNodeType` answers `null` for a type no app owns, and the menu entry
 * is hidden in that case — so this is only ever reached with a real route.
 */
const openInOwner = (row: QueryRow) => {
  const route = routeForNodeType(row.node_type);
  if (!route) return;
  closeMenu();
  router.push({ name: route }).catch(e => logger.warn('Could not open the owning app', e));
};

const renameRow = async (row: QueryRow) => {
  closeMenu();
  await openRow(row);
  await nextTick();
  detailRef.value?.focusTitle();
};

const duplicateRow = async (row: QueryRow) => {
  closeMenu();
  const made = await rowActions.duplicate(row.id);
  await rerun();
  if (made) await openRow({ ...row, id: made, title: '' });
};

/**
 * One question, asked the same way for every removal.
 *
 * Four things in this screen take something away, and only one of them used to
 * ask. The worst of the silent three was not the one anybody noticed: the `×`
 * beside a property really does delete the key from the file, while removing a
 * field from a kind's shape deletes nothing at all — and they looked equally
 * safe, which is to say the dangerous one looked safe.
 *
 * So the message carries the consequence rather than the gesture. "Nothing is
 * deleted from any file" is worth reading once, and a confirmation is the one
 * place somebody is certain to read it.
 */
const confirming = ref<{
  title: string;
  message: string;
  /** The word on the button, which is the last thing read before committing. */
  verb: string;
  run: () => unknown;
} | null>(null);

const askThen = (title: string, message: string, verb: string, run: () => unknown) => {
  confirming.value = { title, message, verb, run };
};

const runConfirmed = async () => {
  const pending = confirming.value;
  confirming.value = null;
  await pending?.run();
};

/**
 * Ask first.
 *
 * The Notes app deletes on the click and offers an undo instead, on the
 * argument that a dialog trains people to click through it. That reasoning
 * holds for a note in the Notes app; it holds less here. This list mixes every
 * type in the vault, the button is a small icon a few pixels from the row's
 * title, and a `book` put back by nobody is far harder to notice missing than
 * a note. So Things asks — and still offers the undo behind it.
 */
/**
 * Take a field off this node. The only one of the four that deletes a value.
 *
 * The patch names the key as `null`, which is the delete in the write
 * contract — the value is gone from the file on the next save.
 */
const askRemoveField = (index: number) => {
  const field = detail.fields.value[index];
  if (!field) return;
  if (!field.key.trim() && !field.value.trim()) {
    // An empty row nobody filled in. Nothing to lose and nothing to ask.
    detail.removeField(index);
    return;
  }
  askThen(
    t('things.remove_field_title'),
    t('things.remove_field_message', { field: field.key || t('things.field_name') }),
    t('things.delete'),
    async () => {
      detail.removeField(index);
      await detail.save();
    },
  );
};

const askRemove = (row: QueryRow) => {
  closeMenu();
  askThen(
    t('things.delete_title'),
    t('things.delete_message', { title: row.title || row.id }),
    t('things.delete'),
    () => confirmRemove(row),
  );
};

const confirmRemove = async (row: QueryRow) => {
  if (selectedId.value === row.id) {
    detail.close();
    links.clear();
    selectedId.value = null;
  }
  await rowActions.remove(row.id, row.title || row.id);
  await rerun();
  await loadTypes();
};

const undoRemove = async () => {
  await rowActions.undoRemove();
  await rerun();
  await loadTypes();
};
const { query, result, loading, error, run, showType, more, loadingMore } = useThingsQuery();
const detail = useThingsNode();
const links = useThingsLinks();

const activeType = ref<string | null>(null);
const showInternal = ref(false);
const selectedId = ref<string | null>(null);
/**
 * Both edges are draggable, the same way Notes' are and through the same
 * composable. The starting widths are the ones this app already had, so
 * nothing moves until somebody pulls it.
 */
const sidebar = useSidebarResize({
  left: { initial: 260, min: 220, max: 600 },
  right: { initial: 300, min: 220, max: 600 },
});

const showRail = ref(true);

/**
 * Nothing but the node.
 *
 * Both rails, not just the chrome: the list on the left and the graph on the
 * right are the two things looking at you while you write.
 */
const zenMode = ref(false);

/**
 * How wide the body runs, for this screen rather than for this node.
 *
 * Notes keeps its equivalent in each note's frontmatter as `full_width`, and
 * that is right for Notes — a note that wants the width wants it every time.
 * Things declines to do the same, because it would mean writing a layout
 * preference into a `book` or an `animal`, inventing an app-owned field on a
 * type whose shape belongs to the person who made it. See `fieldRegistry`,
 * where `full_width` is deliberately scoped to `note` alone.
 */
const contentFullWidth = ref(false);

/** The node whose history is open, if any. */
const historyNodeId = ref<string | null>(null);

/**
 * Export, borrowed whole from Notes.
 *
 * It wants a list of notes and the id of the current one, so it is handed a
 * list of exactly one: the open node, wearing the shape the exporter reads.
 * The alternative was a second export path to keep in step with the first.
 */
const exportShim = computed<NoteItem[]>(() => {
  const open = detail.node.value;
  if (!open) return [];
  const tags = (open.properties as Record<string, unknown> | undefined)?.tags;
  return [{
    id: open.id,
    title: detail.title.value,
    summary: '',
    date: '',
    tags: Array.isArray(tags) ? tags.map(String) : typeof tags === 'string' && tags ? [tags] : [],
    path: open.id,
    pinned: false,
    full_width: false,
  }];
});

/**
 * The body, with the fields in front of it when the fields are the point.
 *
 * A faithful port of the Notes exporter would write `# Title` and the body,
 * which is the whole of a note and almost none of a book: `author`, `rating`
 * and `read_at` live in frontmatter, and an export that drops them hands over
 * a file with the title and a blank page under it.
 *
 * `tags` is left out because the exporter already offers it as its own option,
 * and a note with nothing but tags produces no table at all — so exporting a
 * note through Things gives the same file it gives through Notes.
 */
const exportBody = computed(() => withProperties(detail.body.value, detail.fields.value));

const nodeExport = useNoteExport({
  notes: exportShim as unknown as Ref<NoteItem[]>,
  currentNoteId: selectedId,
  currentContent: exportBody,
  vaultPath: computed(() => props.vaultPath) as unknown as Ref<string>,
});

/**
 * A restored version replaces the body in place.
 *
 * Written straight back rather than left in the editor, because a restore that
 * only changes what is on screen is one navigation away from being lost.
 */
const onVersionRestored = async (content: string) => {
  detail.body.value = content;
  await detail.save();
};

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
  // Opening something is leaving the manager, wherever the click came from.
  // This lived in the manager's own row handler, so a kind picked there left
  // and the same kind picked in the rail did not — the middle of the screen
  // stayed on the list of kinds while the rail beneath it changed, which reads
  // as the app ignoring you.
  viewMode.value = 'things';
  activeType.value = nodeType;
  selectedId.value = null;
  detail.close();
  links.clear();
  typed.value = `type:${nodeType}`;
  arrange.reset();
  // The declared shape first: those are the fields somebody said this kind is
  // about, which is a better answer than whichever keys happen to be common.
  // A kind with no shape, or one emptied on purpose, falls back to the vault.
  const declared = shapeOf(nodeType).map(f => f.key);
  arrange.suggestColumns(declared.length ? declared : fieldsFor(nodeType), nodeType);
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
  viewMode.value = 'things';
  selectedId.value = row.id;
  // The kind's shape travels with the open, so an empty field is drawn the
  // way it was declared rather than as the text box `kindOf('')` implies.
  await Promise.all([detail.open(row.id, shapeOf), links.load(row.id, row.title)]);
};

/** A backlink is a node like any other, so it opens the same way. */
/**
 * Put the node away and go back to whatever was showing it.
 *
 * Escape as well as the button: a pane drawn over another pane is a thing
 * people press Escape at, and being right about that costs one listener.
 */
const closeNode = () => {
  selectedId.value = null;
  detail.close();
  links.clear();
};

/**
 * A link followed from inside a node's body.
 *
 * `[[wikilinks]]`, `synabit://` links and embedded whiteboards all raise the
 * same event, and Things listened for none of it — a link in the body of a
 * `book` looked like a link, took a click, and did nothing. Everything the
 * editor can point at is a node, so it opens the way a backlink does.
 */
const openFromBody = async (id: string, _type: string) => {
  selectedId.value = id;
  await Promise.all([detail.open(id, shapeOf), links.load(id, '')]);
};

const openLinked = async (id: string, title: string) => {
  selectedId.value = id;
  await Promise.all([detail.open(id, shapeOf), links.load(id, title)]);
};

const total = computed(() => result.value?.total ?? 0);

/**
 * How many rows came back, which is not always how many matched.
 *
 * `run_node_query` caps every query at `MAX_QUERY_LIMIT` — 500 — whether or
 * not one was asked for. `total` is a real `COUNT(*)` and reports every match,
 * so a vault with a thousand notes said "1000 results" over a list that
 * stopped at five hundred and gave no sign of it. The number was true and the
 * screen was not.
 */
const shown = computed(() => result.value?.rows.length ?? 0);
const truncated = computed(() => shown.value < total.value);

/**
 * How much of a kind the rail shows, on purpose.
 *
 * The rail is 260px of navigation — it is how you get back to something you
 * were just in. Scrolling is not finding: nobody reads a thousand truncated
 * titles looking for one, they search. So the column stops at a length a
 * person can take in, and says where the rest are.
 *
 * Before this, the rail and the table were handed the same rows and differed
 * only in width, which is why a vault of a thousand notes produced a scroll
 * bar the size of a hair and no way to tell that five hundred were missing.
 */
const RAIL_ROWS = 10;

/**
 * The pinned ones, kept out of the recent list rather than repeated in it.
 *
 * A separate query rather than a filter over the page, because a pinned node
 * is exactly the one that scrolls out of a recent list — it is pinned because
 * it is not recent. Filtering what came back would only find the pinned things
 * that happened to be recent anyway, which is the ones that need it least.
 */
const pinned = ref<QueryRow[]>([]);

/**
 * How many pins there are, which is not how many came back.
 *
 * The rail asks for a page of them; `total` is a real `COUNT(*)` of the
 * matches. Counting the rows instead would have the footer offer "see all 20"
 * over a vault holding forty — the same lie the node list told before its
 * count was separated from its page.
 */
const pinnedTotal = ref(0);

const loadPinned = async () => {
  const scope = activeType.value ? `type:${activeType.value} ` : '';
  try {
    const answer = await invoke<QueryResult>('run_node_query', {
      query: `${scope}pinned:true sort:-updated_at limit:20`,
      offset: 0,
    });
    pinned.value = answer.rows;
    pinnedTotal.value = answer.total;
  } catch {
    // A vault with nothing pinned answers with an error on some paths; an
    // empty rail section is the right way to say "nothing is pinned".
    pinned.value = [];
    pinnedTotal.value = 0;
  }
};

const isPinned = (id: string) => pinned.value.some(r => r.id === id);

const togglePin = async (row: QueryRow) => {
  closeMenu();
  await rowActions.setPinned(row.id, !isPinned(row.id));
  await Promise.all([loadPinned(), rerun()]);
};

const railResult = computed(() => {
  const found = result.value;
  if (!found) return found;
  return found.rows.length <= RAIL_ROWS
    ? found
    : { ...found, rows: found.rows.slice(0, RAIL_ROWS) };
});

/** Whether the rail is showing a sample rather than the lot. */
const railCapped = computed(() => total.value > (railResult.value?.rows.length ?? 0));

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
const detailRef = ref<{
  focusTitle: () => void;
  focusValue: (index: number) => Promise<void>;
} | null>(null);

/** Where the type picker is open, if it is. */
const pickingTypeAt = ref<{ x: number; y: number } | null>(null);

/** Where the field picker is open, if it is. */
const pickingFieldAt = ref<{ x: number; y: number } | null>(null);

/**
 * Add a field by name, and put the cursor in its value.
 *
 * The name is settled by the time this runs — picked from the kind's own keys,
 * or typed once deliberately — so what is left is the part the person came to
 * do, which is say what it holds.
 */
const addFieldNamed = async (key: string) => {
  pickingFieldAt.value = null;
  detail.addField();
  const index = detail.fields.value.length - 1;
  detail.fields.value[index] = { key, value: '', kind: 'text', original: undefined };
  await detailRef.value?.focusValue(index);
};

/**
 * The kind a new thing is, before anybody says otherwise.
 *
 * Whatever is being browsed, because that is what you are looking at.
 * Otherwise the kind this vault holds most of, because the common case should
 * cost nothing. `note` only when the vault is empty and there is nothing to
 * learn from.
 */
const defaultType = computed(() => {
  if (activeType.value) return activeType.value;
  const most = [...browsable.value].sort((a, b) => b.count - a.count)[0];
  return most?.node_type ?? 'note';
});

/**
 * The button makes a thing, not a kind of thing.
 *
 * When a type is already being browsed it is not a question — that is what you
 * are looking at, so one click makes another one and puts the cursor in its
 * name. Asking "what kind?" every time made the button read as though it
 * created types, which it does not: a type exists because a file says so, and
 * the file is what this writes.
 *
 * The question only appears when there is nothing selected to infer from.
 */
const startCreate = async () => {
  // A kind whose content is not a Markdown body is not this screen's to make.
  // Sending somebody to the app that knows its shape beats writing a file that
  // is a whiteboard by every sign except having anything in it.
  const kind = defaultType.value;
  if (isAuthoredElsewhere(kind)) {
    const route = routeForNodeType(kind);
    if (route) router.push(route);
    return;
  }

  selectedId.value = null;
  links.clear();
  detail.startDraft(defaultType.value, shapeOf(defaultType.value));
  await nextTick();
  detailRef.value?.focusTitle();
};

/**
 * Save, or create if there is nothing to save into yet.
 *
 * Both are the blur of the same field. Creating used to happen a step earlier,
 * when the kind was chosen, which is why the vault holds untitled nodes: the
 * file was already written by the time anybody could abandon it. Now the first
 * thing typed is what commits, and a draft left alone leaves nothing.
 */
const saveOrCreate = async () => {
  if (!detail.draft.value) {
    await detail.save();
    return;
  }
  const created = await detail.commitDraft();
  if (!created) return;

  await loadTypes();
  const type = detail.nodeType.value;
  if (activeType.value && activeType.value !== type) openType(type);
  else await rerun();
  selectedId.value = created;
  await links.load(created, detail.title.value);
};

/**
 * The kind's shape as it stands, declared or merely observed.
 *
 * Editing starts from whichever it is, so the first drag on a kind nobody has
 * touched writes down what was already true rather than an empty list — the
 * schema file is born correct.
 */
/**
 * Every kind there is: the ones the vault holds, and the ones designed ahead.
 *
 * A kind used to exist only because a file said `type: x`, which made
 * designing one before writing anything impossible — the shape would be saved
 * and then vanish until the first node landed. A declared schema now puts its
 * kind in the list at zero, so a shape can be worked out first and filled in
 * afterwards, which is how people actually think about a new kind of thing.
 *
 * Observed first and in the vault's own order, so designing a kind never
 * reorders the list somebody is used to.
 */
const kinds = computed(() => {
  const seen = new Set(browsable.value.map(t => t.node_type));
  const designed = schema.schemas.value
    .filter(s => !seen.has(s.nodeType))
    .map(s => ({ node_type: s.nodeType, count: 0, fields: [] }));
  return [...browsable.value, ...designed];
});

/**
 * How many kinds the rail shows before it stops.
 *
 * Ordered by how much of the vault each holds, so the ten it keeps are the ten
 * anybody reaches for. A vault that has been going a while grows kinds the way
 * it grows tags: `abc` from a typo, `book` designed one evening and not used
 * since, and a rail that lists all of them buries the four you live in.
 */
const KINDS_IN_RAIL = 10;

/**
 * Pinned is short by nature, and short is the point of pinning. Five is what
 * the Notes sidebar keeps.
 */
const PINNED_IN_RAIL = 5;

const railKinds = computed(() => kinds.value.slice(0, KINDS_IN_RAIL));
const railPinned = computed(() => pinned.value.slice(0, PINNED_IN_RAIL));

/**
 * A kind you are browsing is never hidden, whatever its rank.
 *
 * Opening `abc` from the manager and finding the rail highlight nowhere on
 * screen is the app losing your place while telling you it has not.
 */
/**
 * A kind being browsed that the cap pushed out of the rail.
 *
 * Only one this list would ever hold: an internal kind is opened from the
 * folded section below and belongs there, and pinning it up here put the app's
 * own storage among the things somebody keeps — with a count of zero, because
 * `kinds` does not contain it to count.
 */
const hiddenActive = computed(() =>
  !!activeType.value
    && kinds.value.some(k => k.node_type === activeType.value)
    && !railKinds.value.some(k => k.node_type === activeType.value),
);

/** Every pin there is, in the screen built to hold them. */
const showAllPinned = () => {
  const scope = activeType.value ? `type:${activeType.value} ` : '';
  typed.value = `${scope}pinned:true`;
  layout.value = 'table';
  rerun();
};

const shapeOf = (nodeType: string): SchemaField[] => {
  const declared = schema.schemaFor(nodeType);
  if (declared) return [...declared.fields];
  // The kind comes from the vault, not from a constant. Writing a shape down
  // for the first time used to declare every field text, `due_date` included.
  return usualFieldsFor(nodeType).map(key => ({ key, kind: kindOfField(nodeType, key) ?? 'text' }));
};

const reshape = async (nodeType: string, next: SchemaField[]) => {
  await schema.save(nodeType, next);
};

const moveShapeField = async (key: string, by: number) => {
  if (!activeType.value) return;
  const fields = shapeOf(activeType.value);
  const from = fields.findIndex(f => f.key === key);
  const to = from + by;
  if (from < 0 || to < 0 || to >= fields.length) return;
  const [row] = fields.splice(from, 1);
  fields.splice(to, 0, row);
  await reshape(activeType.value, fields);
};

/**
 * Out of the shape, and nowhere near out of the files.
 *
 * No confirmation, deliberately, and it is the only removal on this screen
 * without one. Nothing is deleted: the field drops to "Also seen" and one
 * click puts it back. A dialog in front of a reversible act is how people
 * learn to click through dialogs, which is paid for by the ones that guard
 * something real.
 */
const dropShapeField = async (key: string) => {
  const type = activeType.value;
  if (!type) return;
  await reshape(type, shapeOf(type).filter(f => f.key !== key));
};

/**
 * Declare a field nothing carries yet.
 *
 * `adopt` promotes a key the vault already has; this one puts a key into the
 * shape before any node holds it, which is what designing means. The kind is
 * carried too, so the empty box on a new node is drawn as a date picker rather
 * than a text box that happens to want a date.
 */
/**
 * Change how an empty one of this field is drawn. Converts nothing.
 *
 * The nodes that already carry a value keep it exactly as it is: nothing reads
 * the declaration on the way in or out, and `kindOf` takes the kind from the
 * value itself. Declaring `due_date` text does not turn five dates into
 * strings; it turns the next empty box into a text box.
 */
const setShapeKind = async (key: string, kind: FieldKind) => {
  if (!activeType.value) return;
  const fields = shapeOf(activeType.value);
  const at = fields.findIndex(f => f.key === key);
  if (at < 0 || fields[at].kind === kind) return;
  fields[at] = { ...fields[at], kind };
  await reshape(activeType.value, fields);
};

const declareField = async (key: string, kind: FieldKind) => {
  if (!activeType.value) return;
  const clean = key.trim();
  if (!clean) return;
  const fields = shapeOf(activeType.value);
  if (fields.some(f => f.key === clean)) return;
  await reshape(activeType.value, [...fields, { key: clean, kind }]);
};

const adoptShapeField = async (key: string) => {
  if (!activeType.value) return;
  const fields = shapeOf(activeType.value);
  if (fields.some(f => f.key === key)) return;
  await reshape(activeType.value, [
    ...fields,
    { key, kind: kindOfField(activeType.value, key) ?? 'text' },
  ]);
};

/**
 * Which screen the main pane is showing.
 *
 * The Notes manager works this way — a mode that swaps the pane and leaves by
 * a back arrow, with the rail still beside it. Not a modal: this is somewhere
 * you go and stay, sorting and fixing, rather than a question to dismiss.
 */
const viewMode = ref<'things' | 'manager'>('things');

/**
 * Every kind with the numbers the manager sorts and searches on.
 *
 * Built here rather than in the component because both halves live here: what
 * the vault holds comes from `observedFor`, and what a kind is supposed to
 * look like comes from the schemas.
 */
const managedKinds = computed(() =>
  kinds.value.map(entry => ({
    nodeType: entry.node_type,
    count: entry.count,
    observed: observedFor(entry.node_type),
    shape: shapeOf(entry.node_type).map(f => f.key),
    declared: !!schema.schemaFor(entry.node_type),
  })),
);

/**
 * Save a designed kind, and go straight to it.
 *
 * Only the schema is written — no placeholder node, no empty folder. The kind
 * appears in the rail at zero and stays there until somebody makes one, which
 * is what designing ahead is supposed to feel like.
 */
const designKind = async (nodeType: string, fields: SchemaField[]) => {
  await schema.save(nodeType, fields);
  // `openType` leaves the manager on its own now.
  openType(nodeType);
};

/**
 * Everywhere the key could go: what the files carry, and what the shape
 * declares but nothing carries yet. A field can be merged into one that has
 * been designed and never filled in.
 */
const mergeCandidates = computed(() => {
  if (!activeType.value) return [];
  const keys = new Set([
    ...observedFor(activeType.value).map(f => f.key),
    ...shapeOf(activeType.value).map(f => f.key),
  ]);
  keys.delete(merging.value ?? '');
  return [...keys].filter(k => !isAppOwned(activeType.value!, k) && !GOVERNED.has(k));
});

const askRemoveView = (view: SavedView) => {
  askThen(
    t('things.delete_view'),
    t('things.delete_view_message', { name: view.name }),
    t('things.delete'),
    () => saved.remove(view),
  );
};

/** The key somebody proposed merging away, waiting on a target. */
const merging = ref<string | null>(null);

/** The kind somebody proposed removing, waiting on a count and an answer. */
const removingKind = ref<string | null>(null);

/** The kind being renamed, which is the same command wearing its own name. */
const renamingKind = ref<string | null>(null);

/**
 * A kind answers to a different word now.
 *
 * The declaration follows it: a structure filed under `abc` describes nodes
 * that no longer say `abc`, so it is written out under the new name and the
 * old one dropped — unless the destination already had one, which wins,
 * because merging into a kind means joining what it already is.
 */
const afterRenameKind = async (to: string) => {
  const from = renamingKind.value;
  renamingKind.value = null;
  if (!from) return;

  const declared = schema.schemaFor(from);
  if (declared) {
    if (!schema.schemaFor(to)) await schema.save(to, declared.fields);
    await schema.remove(from);
  }

  await loadTypes();
  await loadPinned();
  openType(to);
};

/**
 * A kind is gone: its nodes are in the trash and its declaration with them.
 *
 * The declaration is removed here rather than in the dialog because it is a
 * separate file with a separate write — the backend deletes nodes, and a kind
 * whose nodes are gone but whose schema remains would come back on the next
 * load as a kind with zero things.
 */
const afterDeleteKind = async () => {
  const type = removingKind.value;
  removingKind.value = null;
  if (!type) return;

  if (schema.schemaFor(type)) await schema.remove(type);
  if (activeType.value === type) activeType.value = null;
  await loadTypes();
  await loadPinned();
};

/** The key somebody proposed ending, waiting on a count and an answer. */
const erasing = ref<string | null>(null);

/**
 * A field is gone from the files; take it out of the shape as well.
 *
 * The shape is a separate file and hears nothing about a deletion, so leaving
 * it would have new nodes offering a key that no longer exists anywhere — the
 * schema describing a vault that is not there.
 */
const afterErase = async () => {
  const key = erasing.value;
  erasing.value = null;

  if (key && activeType.value && schema.schemaFor(activeType.value)) {
    const type = activeType.value;
    const fields = shapeOf(type);
    if (fields.some(f => f.key === key)) {
      await reshape(type, fields.filter(f => f.key !== key));
    }
  }

  await loadTypes();
  await rerun();
};

/**
 * A merge finished: the vault changed, so everything read from it is stale.
 *
 * The counts on this very screen are the reason — leaving them showing the old
 * numbers after a merge would be the screen disagreeing with the files it just
 * rewrote.
 */
const afterMerge = async (to: string) => {
  const from = merging.value;
  merging.value = null;

  /*
   * Mend the shape as well as the files.
   *
   * The rename empties the old key out of every node, but a declared shape is
   * a separate file and does not hear about it. Leaving it alone would have
   * new nodes offering a key nothing carries any more, reading `0 / 127` on a
   * screen that is supposed to describe the kind — the schema would be
   * describing a vault that no longer exists.
   *
   * In place rather than appended, because the order is somebody's and merging
   * two fields is not a reason to move a third.
   */
  if (from && activeType.value && schema.schemaFor(activeType.value)) {
    const type = activeType.value;
    const fields = shapeOf(type);
    const at = fields.findIndex(f => f.key === from);
    if (at >= 0) {
      const already = fields.some(f => f.key === to);
      fields.splice(at, 1, ...(already ? [] : [{ ...fields[at], key: to }]));
      await reshape(type, fields);
    }
  }

  await loadTypes();
  await rerun();
};

const chooseType = (type: string) => {
  pickingTypeAt.value = null;
  detail.setDraftType(type, shapeOf(type));
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

/** Escape closes the node before it closes anything else on the screen. */
const onEscape = (event: KeyboardEvent) => {
  if (event.key !== 'Escape') return;
  if (pickingTypeAt.value || pickingFieldAt.value || merging.value || erasing.value) return;
  if (confirming.value || !detail.node.value) return;
  closeNode();
};

onMounted(async () => {
  window.addEventListener('keydown', onEscape);
  window.addEventListener('mousemove', sidebar.onMouseMove);
  window.addEventListener('mouseup', sidebar.onMouseUp);
  await Promise.all([loadTypes(), saved.load(), schema.load(), loadPinned()]);
});

// A listener on `window` outlives the component that added it unless it is
// taken off, and Things is a route somebody leaves and comes back to.
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onEscape);
  window.removeEventListener('mousemove', sidebar.onMouseMove);
  window.removeEventListener('mouseup', sidebar.onMouseUp);
});
</script>

<template>
  <div
    class="h-full flex min-h-0 bg-white dark:bg-[#141416]"
    :class="{ 'cursor-col-resize': sidebar.isDraggingLeft.value || sidebar.isDraggingRight.value }"
  >

    <!-- ── Left: what the vault holds, then what is in it ───── -->
    <aside
      v-if="!zenMode"
      class="flex-shrink-0 relative border-r border-gray-100 dark:border-[#232326]
             flex flex-col min-h-0 bg-gray-50/60 dark:bg-[#101012]
             max-md:!w-[260px]"
      :style="{ width: sidebar.leftWidth.value + 'px' }"
    >
      <!--
        The grip. Invisible until the pointer is on it, because a permanent
        line down the middle of the app would be a border nobody asked for —
        and the cursor already says what this is.

        Wider than the 1px border it sits on: a hairline target is a target
        people miss. Desktop only, since below `md` the sidebar takes the whole
        screen and there is no second pane to take width from.
      -->
      <div
        class="hidden md:block absolute top-0 right-0 w-1.5 h-full z-10
               cursor-col-resize opacity-0 hover:opacity-100 transition-opacity
               hover:bg-black/10 dark:hover:bg-white/10"
        @mousedown.stop="sidebar.startDragLeft($event)"
      ></div>
      <!--
        Search first, because finding one thing is the commonest reason to look
        at a sidebar at all.
        
        The same box as the one over the table, not a second control with its
        own rules: both bind `typed` and both run `runTyped`, so they cannot
        drift apart or disagree about what was asked. One question, shown
        wherever somebody is standing when they want to ask it.
      -->
      <div class="px-3 pt-3 pb-1 flex-shrink-0">
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
      </div>

      <!--
        Types above the list rather than in a column of their own. A fourth
        column would leave the list about 240px on a 1280px screen, and the
        type list is five to ten entries on a real vault — it does not earn
        the width.
      -->
      <div class="px-4 pt-2 pb-2 flex items-center justify-between flex-shrink-0">
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
            @click="viewMode = 'manager'"
            class="p-1 rounded text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
            :title="t('things.manager_title')"
          >
            <Boxes class="w-3.5 h-3.5" />
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
      <div class="max-h-[38%] overflow-y-auto px-2 pb-2 flex-shrink-0">
        <button
          v-for="entry in railKinds"
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

        <!--
          The kind being browsed, when the cap would have hidden it. Opening
          `abc` from the manager and finding no highlight anywhere is the app
          losing your place while telling you it has not.
        -->
        <button
          v-if="hiddenActive && activeType"
          type="button"
          @click="openType(activeType)"
          class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-sm cursor-pointer
                 bg-gray-200/70 dark:bg-white/10 text-[#1c1c1e] dark:text-[#f4f4f5]"
        >
          <component :is="iconForNodeType(activeType)" class="w-4 h-4 flex-shrink-0 text-gray-400" />
          <span class="truncate">{{ activeType }}</span>
          <span class="ml-auto text-xs text-gray-400 dark:text-gray-600 tabular-nums">
            {{ kinds.find(k => k.node_type === activeType)?.count ?? 0 }}
          </span>
        </button>

        <!--
          To the manager, not open in place.
          
          A vault with a thousand kinds would have answered "show all" with
          nine hundred and ninety rows in a 260px column, which is the same
          problem moved rather than solved. Every capped section in this rail
          works the same way now: a count, and a door to the screen built to
          hold them — the Notes sidebar has done it this way all along.
        -->
        <button
          v-if="kinds.length > KINDS_IN_RAIL"
          type="button"
          @click="viewMode = 'manager'"
          class="w-full px-2 py-1.5 text-left rounded-lg text-xs cursor-pointer
                 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300
                 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
        >
          {{ t('things.see_all', { total: kinds.length }) }} →
        </button>

        <p
          v-if="!typesLoading && kinds.length === 0"
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
              @click="askRemoveView(view)"
              class="p-0.5 rounded text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100
                     focus:opacity-100 transition-all cursor-pointer"
              :title="t('things.delete_view')"
            >
              <Trash2 class="w-3 h-3" />
            </button>
          </div>
        </template>
      </div>

      <!--
        The count, where the rail still owes one. The query box that used to
        sit here has moved above the table: it described a query, and the rail
        describes recency now — two boxes that looked identical and did
        different things was already a complaint.
      -->
      <p
        v-if="result && layout === 'list'"
        class="px-3.5 pt-2 text-[11px] text-gray-400 tabular-nums flex-shrink-0"
      >
        {{ t('things.n_results', { n: total }) }}
      </p>

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

      <!--
        Pinned first, then recent — the shape the Notes sidebar has.
        
        Two sections rather than one sorted list, because a pinned node is
        precisely the one that has fallen out of recency. Mixing them would put
        the thing you pinned back where you pinned it *from*.
      -->
      <div v-if="layout === 'list' && pinned.length" class="flex-shrink-0 px-2 pt-2">
        <h3 class="px-1.5 pb-1 text-[10px] font-semibold uppercase tracking-wider text-gray-400">
          {{ t('things.pinned_section') }}
        </h3>
        <button
          v-for="row in railPinned"
          :key="row.id"
          type="button"
          @click="openRow(row)"
          class="w-full flex items-center gap-2 px-1.5 py-1.5 rounded-lg text-left cursor-pointer
                 transition-colors"
          :class="row.id === selectedId
            ? 'bg-gray-200/70 dark:bg-white/10'
            : 'hover:bg-gray-100 dark:hover:bg-white/5'"
        >
          <Pin class="w-3 h-3 flex-none text-amber-500" />
          <span class="min-w-0 truncate text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5]">
            {{ row.title || t('things.untitled') }}
          </span>
        </button>

        <button
          v-if="pinnedTotal > PINNED_IN_RAIL"
          type="button"
          @click="showAllPinned"
          class="w-full px-1.5 py-1.5 text-left rounded-lg text-xs cursor-pointer
                 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300
                 hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
        >
          {{ t('things.see_all', { total: pinnedTotal }) }} →
        </button>
      </div>

      <h3
        v-if="layout === 'list' && pinned.length"
        class="px-3.5 pt-3 pb-1 text-[10px] font-semibold uppercase tracking-wider
               text-gray-400 flex-shrink-0"
      >
        {{ t('things.recent_section') }}
      </h3>

      <ListView
        v-if="layout === 'list'"
        class="flex-1 min-h-0 border-t border-gray-100 dark:border-[#232326]"
        :result="railResult"
        :loading="loading"
        :selected-id="selectedId"
        :group-by="arrange.groupBy.value"
        :untitled-label="t('things.untitled')"
        :menu-for="menuRow?.id ?? null"
        @open="openRow"
        @menu="setMenu"
      />

      <!--
        Where the rest are. The rail stopping short is only honest if it says
        so, and the table is the screen that can actually hold them.
      -->
      <button
        v-if="layout === 'list' && railCapped"
        type="button"
        @click="layout = 'table'"
        class="flex-shrink-0 w-full px-3 py-2 text-left text-[11px] cursor-pointer
               border-t border-gray-100 dark:border-[#232326]
               text-gray-500 dark:text-gray-400
               hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
      >
        {{ t('things.see_all', { total }) }} →
      </button>
      <div v-else class="flex-1 min-h-0 border-t border-gray-100 dark:border-[#232326]"></div>
    </aside>

    <!-- ── Middle: the thing itself ─────────────────────────── -->
    <SchemaManager
      v-if="viewMode === 'manager'"
      :kinds="managedKinds"
      @open="openType"
      @remove="removingKind = $event"
      @rename="renamingKind = $event"
      @create="designKind"
      @close="viewMode = 'things'"
    />

    <section v-else class="flex-1 flex flex-col min-w-0 min-h-0 relative">
      <!--
        The table wants the width the list does not. It sits here rather than
        in the left column, and the node opens over it — which is how the Tasks
        board has always behaved, so it is a habit rather than a new rule.
      -->
      <!--
        The table, under the same header the shape page carries.
        
        One click on a kind used to land on two different screens depending on
        a toggle set earlier, possibly on another kind: the layout switch says
        where the list is drawn, and it decided which page the middle showed as
        a side effect. The state was real and invisible. Now both pages wear
        the same bar and each names the other, so which one you are on — and
        how to reach the other — is on screen rather than inferred.
      -->
      <div v-if="layout === 'table' && result" class="flex-1 flex flex-col min-h-0">
        <header
          v-if="activeType"
          class="flex items-center gap-3 px-6 h-11 shrink-0 border-b
                 border-gray-100 dark:border-[#232326]"
        >
          <button
            type="button"
            @click="viewMode = 'manager'"
            class="p-1.5 -ml-1.5 rounded-md text-gray-500 cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
            :aria-label="t('things.back_to_kinds')"
            :title="t('things.back_to_kinds')"
          >
            <ArrowLeft class="w-4.5 h-4.5" />
          </button>
          <component :is="iconForNodeType(activeType)" class="w-4 h-4 text-gray-400 flex-none" />
          <!--
            The app's own token classes, and no `font-mono`.
            
            The manager's header renders its title exactly this way; this one
            differed in both and came out as a gap between the icon and the
            count — a word occupying its width and showing nothing.
          -->
          <h1 class="text-base font-semibold text-text dark:text-text-dark">
            {{ activeType }}
          </h1>
          <span
            class="text-[11px] font-medium px-2 py-0.5 rounded-full tabular-nums
                   bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400"
          >
            {{ total }}
          </span>

          <div class="flex-1" />

          <button
            type="button"
            @click="layout = 'list'"
            class="px-2.5 py-1 rounded-md text-xs cursor-pointer
                   text-gray-500 dark:text-gray-400
                   hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
          >
            {{ t('things.see_shape') }}
          </button>
        </header>

        <!--
          The query box, above the table it fills.
          
          It used to be in the rail beside a plain text filter that looked
          identical and behaved nothing like it. Here there is one box on the
          screen, it sits over the rows it decides, and `Enter` runs it.
        -->
        <div class="px-4 pt-4 flex items-center gap-2 flex-shrink-0">
          <div class="relative flex-1">
            <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400 pointer-events-none" />
            <input
              v-model="typed"
              type="text"
              spellcheck="false"
              @keydown.enter="runTyped"
              :placeholder="t('things.query_placeholder')"
              class="w-full pl-9 pr-3 py-2 rounded-lg text-sm outline-none
                     bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700
                     text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400
                     focus:border-violet-400 dark:focus:border-violet-500/50 transition-colors"
            />
          </div>
          <p
            v-if="result"
            class="text-[11px] tabular-nums whitespace-nowrap"
            :class="truncated ? 'text-amber-700 dark:text-amber-500' : 'text-gray-400'"
            :title="truncated ? t('things.showing_hint') : ''"
          >
            {{ truncated ? t('things.showing_some', { shown, total }) : t('things.n_results', { n: total }) }}
          </p>
        </div>

        <div class="flex-1 overflow-auto min-h-0 p-4">
        <TableView
          :result="result"
          :selected-id="selectedId"
          :untitled-label="t('things.untitled')"
          @open="openRow"
        />

        <!--
          The rest of them, a page at a time.
          
          Appending rather than replacing, so a sort or a group keeps seeing
          one list — and so scrolling back up still lands on what you read.
        -->
        <div v-if="truncated" class="flex items-center gap-3 pt-4 pb-2">
          <button
            type="button"
            :disabled="loadingMore"
            @click="more"
            class="px-3 py-1.5 rounded-lg text-xs font-medium cursor-pointer
                   border border-gray-200 dark:border-gray-700
                   text-gray-600 dark:text-gray-300
                   hover:bg-gray-100 dark:hover:bg-white/5 disabled:opacity-50"
          >
            {{ loadingMore ? t('things.loading_more') : t('things.load_more') }}
          </button>
          <span class="text-[11px] text-gray-400 tabular-nums">
            {{ t('things.showing_some', { shown, total }) }}
          </span>
        </div>
        </div>
      </div>

      <!--
        Browsing a kind with nothing open: the middle shows the kind itself.
        This space used to hold one line of placeholder text, and it is already
        where somebody stands when they are thinking about the kind rather than
        about one of its members.
      -->
      <TypeOverview
        v-else-if="!detail.node.value && activeType && !isInternalType(activeType)"
        :node-type="activeType"
        :count="types.find(t => t.node_type === activeType)?.count ?? 0"
        :fields="observedFor(activeType)"
        :usual="shapeOf(activeType).map(f => f.key)"
        :declared="!!schema.schemaFor(activeType)"
        :kinds="Object.fromEntries(shapeOf(activeType).map(f => [f.key, f.kind]))"
        :observed-kinds="Object.fromEntries(
          observedFor(activeType)
            .map(f => [f.key, kindOfField(activeType!, f.key)])
            .filter(([, k]) => !!k),
        )"
        @set-kind="setShapeKind"
        @move="moveShapeField"
        @drop="dropShapeField"
        @adopt="adoptShapeField"
        @rename="merging = $event"
        @erase="erasing = $event"
        @back="viewMode = 'manager'"
        @remove-kind="removingKind = activeType"
        @rename-kind="renamingKind = activeType"
        @browse="layout = 'table'"
        @add-field="declareField"
      />

      <!--
        The app's own storage, opened from the folded section below the list.
        
        There is a page for managing a kind, and it does not apply here: these
        files are written and read by Synabit, and renaming `schema` or
        deleting `json` breaks the app rather than tidying a vault. They are
        listed at all because looking is reasonable and hiding what exists is
        not — so looking is what this offers, and nothing else.
      -->
      <div
        v-else-if="!detail.node.value && activeType && isInternalType(activeType)"
        class="flex-1 flex flex-col min-h-0"
      >
        <header
          class="flex items-center gap-3 px-6 h-11 shrink-0
                 border-b border-gray-100 dark:border-[#232326]"
        >
          <component :is="iconForNodeType(activeType)" class="w-4 h-4 flex-none text-gray-400" />
          <h1 class="font-mono text-sm text-[#1c1c1e] dark:text-[#f4f4f5]">{{ activeType }}</h1>
          <span
            class="text-[11px] font-medium px-2 py-0.5 rounded-full
                   bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400 tabular-nums"
          >
            {{ types.find(t => t.node_type === activeType)?.count ?? 0 }}
          </span>
        </header>

        <div class="flex-1 flex items-center justify-center px-8 text-center">
          <div class="max-w-sm space-y-4">
            <p class="text-sm text-gray-400 dark:text-gray-500 leading-relaxed">
              {{ t('things.internal_kind_what', { type: activeType }) }}
            </p>
            <button
              v-if="types.find(t => t.node_type === activeType)?.count"
              type="button"
              @click="layout = 'table'"
              class="px-3 py-1.5 rounded-md text-xs text-gray-600 dark:text-gray-300
                     border border-gray-200 dark:border-gray-700 cursor-pointer
                     hover:bg-gray-100 dark:hover:bg-white/5"
            >
              {{ t('things.browse_hint', { type: activeType }) }}
            </button>
          </div>
        </div>
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
<!--
          The same row Notes carries, minus the one that does not travel.
          Every button here works on a node of any kind: a history is a
          history, a width is a width. Pinning stayed behind in Notes for the
          reason it stayed out of the row menu.
        -->
        <!--
          The way out, and only where there is not one already.
          
          In the list layout the rail beside you is the way back. In the table
          layout the node is drawn over the table and the rail draws no list at
          all, so opening one was a door that closed behind you.
        -->
        <button
          v-if="layout === 'table'"
          type="button"
          @click="closeNode"
          class="absolute top-3 left-3 z-30 flex items-center gap-1.5 px-2 py-1.5 rounded-md
                 text-xs text-gray-500 dark:text-gray-400 cursor-pointer
                 bg-white/85 dark:bg-[#141416]/85 backdrop-blur
                 hover:bg-gray-100 dark:hover:bg-white/10 transition-colors"
        >
          <ArrowLeft class="w-3.5 h-3.5" />
          {{ t('things.back_to_all') }}
        </button>

        <div class="absolute top-3 right-3 z-30 flex items-center gap-1">
          <button
            type="button"
            @click="zenMode = !zenMode"
            class="p-1.5 rounded-md transition-colors cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5"
            :class="zenMode ? 'text-blue-500' : 'text-gray-400'"
            :title="zenMode ? t('things.zen_exit') : t('things.zen')"
          >
            <Monitor class="w-4 h-4" />
          </button>

          <button
            type="button"
            @click="contentFullWidth = !contentFullWidth"
            class="p-1.5 rounded-md transition-colors cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5"
            :class="contentFullWidth ? 'text-blue-500' : 'text-gray-400'"
            :title="contentFullWidth ? t('things.width_standard') : t('things.width_full')"
          >
            <div v-if="contentFullWidth" class="flex items-center space-x-[1px]">
              <ArrowRight class="w-3 h-3" />
              <ArrowLeft class="w-3 h-3" />
            </div>
            <div v-else class="flex items-center space-x-[1px]">
              <ArrowLeft class="w-3 h-3" />
              <ArrowRight class="w-3 h-3" />
            </div>
          </button>

          <button
            type="button"
            @click="historyNodeId = selectedId"
            class="p-1.5 rounded-md text-gray-400 transition-colors cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5"
            :title="t('things.history')"
          >
            <History class="w-4 h-4" />
          </button>

          <button
            type="button"
            @click="nodeExport.exportModalVisible.value = true"
            class="p-1.5 rounded-md text-gray-400 transition-colors cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5"
            :title="t('things.export')"
          >
            <Download class="w-4 h-4" />
          </button>

          <button
            type="button"
            @click="showRail = !showRail"
            class="p-1.5 rounded-md text-gray-400 transition-colors cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5"
            :title="t('things.toggle_rail')"
          >
            <PanelRightClose v-if="showRail && !zenMode" class="w-4 h-4" />
            <PanelRight v-else class="w-4 h-4" />
          </button>
        </div>

        <ObjectDetail
          ref="detailRef"
          :class="layout === 'table'
            ? 'absolute inset-0 z-20 border-l border-gray-200 dark:border-[#2c2c2c] shadow-2xl'
            : ''"
          v-model:title="detail.title.value"
          v-model:body="detail.body.value"
          :node-id="selectedId ?? undefined"
          :zen-mode="zenMode"
          @open-node="openFromBody"
          v-model:fields="detail.fields.value"
          :node-type="detail.nodeType.value"
          :read-only-rows="detail.readOnlyRows.value"
          :app-fields="detail.appFields.value"
          :loading="detail.loading.value"
          :saving="detail.saving.value"
          :vault-path="props.vaultPath"
          :authored-in="isAuthoredElsewhere(detail.nodeType.value)
            ? appName(routeForNodeType(detail.nodeType.value) ?? '')
            : null"
          @open-owner="openInOwner({ id: selectedId ?? '', node_type: detail.nodeType.value, title: detail.title.value, cells: [] })"
          :full-width="contentFullWidth"
          :type-editable="!!detail.draft.value"
          @save="saveOrCreate"
          @pick-type="pickingTypeAt = $event"
          @pick-field="pickingFieldAt = $event"
          @add-field="detail.addField"
          @remove-field="askRemoveField"
        />
      </template>
    </section>

    <!-- ── Right: where this sits in the graph ──────────────── -->
    <aside
      v-if="viewMode === 'things' && detail.node.value && showRail && !zenMode"
      class="flex-shrink-0 relative border-l border-gray-100 dark:border-[#232326]
             flex flex-col min-h-0 bg-[#fbfbfc] dark:bg-[#101012]
             max-md:!w-[300px]"
      :style="{ width: sidebar.rightWidth.value + 'px' }"
    >
      <div
        class="hidden md:block absolute top-0 left-0 w-1.5 h-full z-10
               cursor-col-resize opacity-0 hover:opacity-100 transition-opacity
               hover:bg-black/10 dark:hover:bg-white/10"
        @mousedown.stop="sidebar.startDragRight"
      ></div>
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
    <!--
      Clicking anywhere else closes the menu. Behind it in the stack, so it
      never swallows a click meant for the menu itself.
    -->
    <div v-if="menuRow" class="fixed inset-0 z-[60]" @click="closeMenu"></div>

    <NodeRowMenu
      v-if="menuRow && menuAt"
      :node-id="menuRow.id"
      :node-type="menuRow.node_type"
      :at="menuAt"
      :pinned="isPinned(menuRow.id)"
      @open="openInOwner(menuRow)"
      @rename="renameRow(menuRow)"
      @duplicate="duplicateRow(menuRow)"
      @copy-path="rowActions.copyPath(menuRow.id); closeMenu()"
      @pin="togglePin(menuRow)"
      @remove="askRemove(menuRow)"
    />

    <TypePicker
      v-if="pickingTypeAt"
      :types="kinds.filter(k => !isAuthoredElsewhere(k.node_type))"
      :current="detail.nodeType.value"
      :at="pickingTypeAt"
      @pick="chooseType"
      @close="pickingTypeAt = null"
    />

    <RenameKindDialog
      v-if="renamingKind"
      :vault-path="props.vaultPath"
      :node-type="renamingKind"
      :candidates="kinds.map(k => k.node_type).filter(k => k !== renamingKind)"
      @done="afterRenameKind"
      @close="renamingKind = null"
    />

    <RemoveKindDialog
      v-if="removingKind"
      :vault-path="props.vaultPath"
      :node-type="removingKind"
      :declared="!!schema.schemaFor(removingKind)"
      :candidates="kinds.map(k => k.node_type).filter(k => k !== removingKind)"
      @done="afterDeleteKind"
      @close="removingKind = null"
    />

    <DeleteFieldDialog
      v-if="erasing && activeType"
      :vault-path="props.vaultPath"
      :node-type="activeType"
      :field="erasing"
      @done="afterErase"
      @close="erasing = null"
    />

    <RenameFieldDialog
      v-if="merging && activeType"
      :vault-path="props.vaultPath"
      :node-type="activeType"
      :from="merging"
      :candidates="mergeCandidates"
      @done="afterMerge"
      @close="merging = null"
    />

    <FieldPicker
      v-if="pickingFieldAt"
      :known="observedFor(detail.nodeType.value)"
      :taken="detail.fields.value.map(f => f.key)"
      :at="pickingFieldAt"
      @pick="addFieldNamed"
      @close="pickingFieldAt = null"
    />

    <NoteHistoryModal
      v-if="historyNodeId"
      :vault-path="props.vaultPath"
      :note-id="historyNodeId"
      :note-title="detail.title.value || t('things.untitled')"
      @close="historyNodeId = null"
      @restored="onVersionRestored"
    />

    <NoteExportModal
      v-if="nodeExport.exportModalVisible.value"
      @close="nodeExport.exportModalVisible.value = false"
      @export="(o: any) => nodeExport.handleExportOption(o)"
    />

    <ConfirmModal
      :show="!!confirming"
      :title="confirming?.title ?? ''"
      :message="confirming?.message ?? ''"
      :confirm-text="confirming?.verb ?? t('things.delete')"
      :cancel-text="t('things.cancel')"
      is-destructive
      @confirm="runConfirmed"
      @cancel="confirming = null"
    />

    <UndoToast
      :show="!!rowActions.trashed.value"
      :restart-key="rowActions.trashed.value?.key"
      :message="t('things.deleted', { title: rowActions.trashed.value?.title ?? '' })"
      :undo-label="t('things.undo')"
      :seconds="UNDO_WINDOW_SECONDS"
      @undo="undoRemove"
    />
  </div>
</template>
