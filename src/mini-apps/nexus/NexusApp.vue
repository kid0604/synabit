<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { useEventBus } from '../../composables/useEventBus';
import { Search, FileText, CheckSquare, Zap, X, ChevronRight, Tag, File, Calendar, PenTool, Users, Lock, Scale, Share2, CalendarDays, Sparkles } from 'lucide-vue-next';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import GraphView from './components/GraphView.vue';
import DatedView from '../../shared/views/DatedView.vue';
import EventsOverTime from '../../shared/views/EventsOverTime.vue';
import TimeWindowPicker from '../../shared/views/TimeWindowPicker.vue';
import { autoWindow, outside, windowRange, within, type DayRange, type WindowChoice } from '../../shared/views/overTime';
import type { QueryResult, QueryRow } from '../../shared/views/types';
import ExtractTray from './components/ExtractTray.vue';
import NexusTagManager from './components/NexusTagManager.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import EventCompose from '../../shared/components/EventCompose.vue';
import { logger } from '../../utils/logger';
import { refusalText } from '../../shared/refusal';
import { onSource, withRoomFor } from '../../shared/queryChips';
import { localDay } from '../../shared/localDay';
import { useSidebarResize } from '../../composables/useSidebarResize';
import { useAppLockStore } from '../../stores/useAppLockStore';
import { useAppStore } from '../../stores/useAppStore';
import { storeToRefs } from 'pinia';

const bus = useEventBus();

const emit = defineEmits<{
    /**
     * `query` is what the reader typed to find this. Files carry it through so
     * a document can open on the page the phrase is actually on, rather than at
     * page one.
     */
    (e: 'edit-item', id: string, type: string, query?: string): void
}>();

const props = defineProps<{
    vaultPath: string;
}>();

interface NexusItem {
    id: string;
    item_type: string;
    title: string;
    preview: string;
    tags: string[];
    date: string;
    path: string;
    content: string;
    status?: string;
}

interface SearchResult {
    id: string;
    item_type: string;
    title: string;
    snippet: string;
    tags: string[];
    date: string;
    path: string;
    score: number;
    status?: string;
}

interface SearchResponse {
    results: SearchResult[];
    total_count: number;
    query_time_ms: number;
}

interface GraphNode {
    id: string;
    item_type: string;
    title: string;
    tags: string[];
}

interface GraphLink {
    source: string;
    target: string;
}

interface GraphData {
    nodes: GraphNode[];
    links: GraphLink[];
}

const allItems = ref<NexusItem[]>([]);
const graphData = ref<GraphData | null>(null);
/**
 * Ids the current query matched, for the graph to narrow itself to. Null when
 * nothing is being searched, which is not the same as an empty array — that
 * one means the query matched nothing, and the graph should say so by being
 * empty rather than by showing everything.
 */
const graphMatchIds = ref<string[] | null>(null);
const searchResults = ref<SearchResult[]>([]);
/**
 * Why the search box will not answer, when it will not.
 *
 * It used to be `logger.error(String(e))` and nothing on screen: a question
 * this box cannot ask — a `|` step, a timeline word, an `OR` — left the last
 * answer sitting there, or nothing at all. The engine refuses precisely and
 * says where the question *can* be asked, and none of that reached anybody.
 */
const searchRefused = ref<string | null>(null);

/**
 * Which way the pane draws the vault **when nothing has been asked**.
 *
 * One vault, two shapes: the graph answers *what is connected to what*, the
 * timeline answers *when*. This is how somebody browses without a question.
 *
 * It does not survive a search, and that is the point. While a question is
 * open the answer is read in the two tabs beside it, and the word "Timeline"
 * appeared **twice on one screen** — once as a tab, once as this — drawing
 * the same rows in both. Two controls for one thing is worse than either.
 * So a search takes the pane back to the graph, which is the one thing the
 * tabs cannot show.
 */
const shownAs = ref<'graph' | 'timeline'>('graph');

/**
 * What the pane draws.
 *
 * With a question open, it **follows the tab**: nodes are drawn as the graph,
 * because for a thing what matters is what it is connected to; events are
 * drawn across time, because for something that happened what matters is
 * when. The pane used to show the graph whichever tab was open — and the
 * graph is built from the *node* matches, so on the events tab the two halves
 * of the screen were answering two different questions.
 */
const paneShows = computed<'graph' | 'timeline' | 'events'>(() =>
    searchQuery.value ? (searchTab.value === 'events' ? 'events' : 'graph') : shownAs.value,
);

/**
 * The stretch of time the events list is narrowed to, picked on the chart.
 *
 * Forgotten when the question changes: a range picked for one answer, still
 * applied to the next, would hide rows of it for a reason nobody can see.
 */
const timeRange = ref<DayRange | null>(null);

/**
 * The stretch of time the timeline is looked at through.
 *
 * `null` until somebody picks one, and until then the answer decides: all of
 * it when there is little, the last year when there is a lot (`autoWindow`).
 * A vault holding a few things from 1991 and two hundred from this year drew
 * this year as a single column at the edge of a thirty-five-year axis.
 *
 * Kept across questions once picked — it is how somebody wants to look, not
 * part of what they asked — and it scopes the chart and the list alike, so
 * the two can never be counting different stretches.
 */
const windowPicked = ref<WindowChoice | null>(null);

/** The events the window applies to: the question's, or the vault's own. */
const timelineSource = computed(() => (searchQuery.value ? eventsAnswer.value : wholeTimeline.value));
const windowChoice = computed<WindowChoice>(() => windowPicked.value ?? autoWindow(timelineSource.value, new Date()));
const windowDays = computed(() => windowRange(windowChoice.value, new Date()));
const windowed = computed(() => within(timelineSource.value, windowDays.value));
const windowHidden = computed(() => outside(timelineSource.value, windowDays.value));

/** A new window lets go of any stretch picked inside the old one. */
const pickWindow = (choice: WindowChoice) => {
    windowPicked.value = choice;
    timeRange.value = null;
};

/** The whole timeline, for when nothing has been asked. */
const wholeTimeline = ref<QueryResult | null>(null);

/**
 * Which half of the vault the answer is being read on.
 *
 * One box, two tables. What was typed is asked of **both** — of `nodes`, which
 * is every thing the vault holds, and of `events`, which is what happened —
 * and the two answers sit in two tabs rather than one pretending to be the
 * other.
 *
 * It used to pretend. The timeline, while a search was open, drew
 * `asTimeline(searchResults)`: the **node** hits laid out on the day each node
 * carries. So the same word on the same screen meant a thing on one side and
 * an event on the other, and nothing said so. A vault that holds both has to
 * answer as both.
 */
const searchTab = ref<'nodes' | 'events'>('nodes');

/**
 * The proposals waiting to be looked at, and the screen for looking at them.
 *
 * # Where this sits, and why
 *
 * Three things decide it, and none of them is "wherever there was room":
 *
 * 1. **It fills while nobody is watching.** `App.vue` reads notes into
 *    proposals every ten minutes, in the background. A queue that grows on its
 *    own has to say so where somebody will see it, so the count lives next to
 *    the search box — the one part of this screen that is always there.
 * 2. **Reviewing is a long, repetitive read.** Each row is a sentence, a day,
 *    a list of people and the line it came from, and there can be dozens. It
 *    used to be a 384px popover, opened from a row of buttons inside another
 *    popover, behind a button called "Look back" — three presses away from a
 *    screen it has nothing to do with. It gets the whole window now.
 * 3. **Its output is the timeline.** So the line on the timeline tab that
 *    owns up to being partial opens this same screen: seeing that the answer
 *    is short and being able to do something about it belong together.
 */
const proposals = ref<{ waiting: number; unread: number } | null>(null);
const reviewing = ref(false);

const loadProposals = async () => {
    try {
        const status = await invoke<{ pending: number; proposals: unknown[] }>(
            'timeline_extract_status',
            { vaultPath: props.vaultPath },
        );
        proposals.value = { waiting: status.proposals.length, unread: status.pending };
    } catch (e) {
        // A count nobody can read is not worth an error in front of whatever
        // they were doing; the badge simply does not appear.
        logger.error('Could not count what is waiting to be reviewed', e);
    }
};

/**
 * Something changed what the timeline holds.
 *
 * One handler for both doors — a proposal kept and an event written by hand
 * are the same news — so the timeline in the pane, the answer in the tab and
 * the count on the button can never be three different opinions.
 */
const eventsChanged = async () => {
    wholeTimeline.value = null;
    if (searchQuery.value.trim()) await performSearch();
    else if (paneShows.value === 'timeline') await loadWholeTimeline();
    await loadProposals();
};

/**
 * What the timeline does not know yet, so its count can be read honestly.
 *
 * A count on that tab is a **floor, not a total**. The vault held a note
 * saying «Chứng kiến Cam tập đi» and the timeline answered `1` for
 * `cam đi học`, because nothing had read that note into an event yet — and
 * nothing on the screen said so. A number nobody can tell is partial is the
 * worst kind, which is the same rule `QueryResult.note` already follows.
 *
 * Asked once, and only when somebody opens that tab: `timeline_extract_status`
 * walks the vault's inputs, and nobody reading the node side should pay for
 * it. `null` means not asked.
 */
/** Pressing a tab. */
const showTab = (tab: 'nodes' | 'events') => {
    searchTab.value = tab;
};

/** What the question matched on the timeline, and why it could not be asked. */
const eventsAnswer = ref<QueryResult | null>(null);
const eventsRefused = ref<string | null>(null);

const loadWholeTimeline = async () => {
    if (wholeTimeline.value) return;
    try {
        wholeTimeline.value = await invoke<QueryResult>('run_node_query', {
            vaultPath: props.vaultPath,
            // As many as the engine gives: the chart above the list is
            // drawn from these, and a page of the newest would say every
            // busy month was a recent one.
            query: 'moments sort:-when limit:1000',
            offset: 0,
        });
    } catch (e) {
        searchRefused.value = refusalText(e);
        logger.error('Could not read the timeline', e);
    }
};

const showAs = async (shape: 'graph' | 'timeline') => {
    shownAs.value = shape;
    if (shape === 'timeline' && !searchQuery.value) await loadWholeTimeline();
};

/** A row of the timeline opens the thing behind it, as the list does. */
const openFromTimeline = (row: QueryRow) =>
    emit('edit-item', row.open ?? row.id, row.node_type);
const searchQuery = ref('');
const isSearching = ref(false);
const queryTimeMs = ref(0);
const totalCount = ref(0);
const showSyntaxHints = ref(false);
const caseSensitive = ref(false);
const currentView = ref('graph_search'); // 'graph_search' | 'tag_manager'

/**
 * How wide the answer column is, and dragging its edge to change it.
 *
 * Three things have to agree about this number: the column, the pane beside
 * it, and the omnibar that steps aside for it. They agreed by each spelling
 * out `sm:left-[420px] lg:left-[480px]`, which is the same number written
 * three times — so it becomes one CSS variable on their common ancestor, and
 * the breakpoint stays in CSS where it belongs. Below `sm` the column is the
 * whole screen and there is no edge to drag, which `sm:w-(--answers)` says
 * without any JavaScript needing to know the window's width.
 *
 * The shared resizer rather than a fourth implementation of dragging: it
 * already owns the clamp and the rule for a button released outside the
 * window. Remembered per device, because re-dragging this every launch is
 * exactly the kind of small tax nobody reports.
 */
const ANSWERS = { initial: 480, min: 320 };

/** What the graph keeps no matter how wide the answers get. */
const ROOM_FOR_THE_PANE = 360;

/** The screen the two of them share, measured rather than assumed. */
const screen = ref<HTMLElement | null>(null);

const answers = useSidebarResize({
    left: {
        ...ANSWERS,
        // Read when asked, not fixed: a ceiling of 900 is right on a big
        // display and broken on a laptop, where dragging all the way over
        // left the graph 224px on a 1024px window. Measured off the element
        // the two panes actually share, so the icon rail's width is never
        // written down a second time.
        max: () =>
            Math.max(ANSWERS.min, (screen.value?.clientWidth ?? window.innerWidth) - ROOM_FOR_THE_PANE),
        remember: 'nexus.answers.width',
    },
});

/**
 * Taking hold of the edge.
 *
 * The capture is the part that is easy to leave out and hard to notice: once
 * the pointer is over the graph, cytoscape would otherwise see every move as
 * a pan and the drag would fight the canvas. Capturing routes them to the
 * handle instead, and they still reach the window listener by bubbling.
 */
const beginResize = (event: PointerEvent) => {
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
    answers.startDragLeft(event);
};

/**
 * Arrow keys move the edge too, so it is not a pointer-only control.
 *
 * It moves and then asks for the same clamp the drag uses, rather than
 * writing the bounds out a second time here. Two clamps is two chances for
 * the keyboard and the pointer to stop at different places.
 */
const nudgeEdge = (by: number) => {
    answers.leftWidth.value += by;
    answers.reclamp();
};

const appLockStore = useAppLockStore();
const { dailyNoteFormat, dailyNoteTag } = storeToRefs(useAppStore());

const hideSyntaxHints = () => {
    setTimeout(() => {
        showSyntaxHints.value = false;
    }, 200);
};

const selectedItem = ref<NexusItem | null>(null);

let searchTimeout: ReturnType<typeof setTimeout>;

const loadAllData = async () => {
    try {
        const [items, data] = await Promise.all([
            invoke<NexusItem[]>('get_nexus_items', { vaultPath: props.vaultPath }),
            invoke<GraphData>('get_nexus_graph_data', { vaultPath: props.vaultPath })
        ]);
        allItems.value = items;
        graphData.value = data;
    } catch (e) {
        logger.error("Failed to load nexus data", e);
    }
};

let currentSearchId = 0;

const performSearch = async () => {
    if (!searchQuery.value.trim()) {
        currentSearchId++;   // strand any answer still in flight
        searchResults.value = [];
        totalCount.value = 0;
        queryTimeMs.value = 0;
        graphMatchIds.value = null;
        eventsAnswer.value = null;
        eventsRefused.value = null;
        return;
    }

    isSearching.value = true;
    searchRefused.value = null;
    eventsRefused.value = null;
    const searchId = ++currentSearchId;
    // The timeline half, asked at the same time and refused on its own.
    //
    // Its own `try` because the two halves fail separately and neither
    // failure is the other's: `#gia-đình` is a question `events` cannot
    // answer, and `when:2019` is one this search box cannot. Sharing a
    // `catch` would let either one empty the other's tab.
    const askedOfEvents = withRoomFor(onSource(searchQuery.value, 'moments'), EVENTS_AT_MOST);
    invoke<QueryResult>('run_node_query', {
        vaultPath: props.vaultPath,
        query: askedOfEvents,
        offset: 0,
    })
        .then(answer => {
            if (searchId === currentSearchId) eventsAnswer.value = answer;
        })
        .catch(e => {
            if (searchId === currentSearchId) {
                eventsRefused.value = refusalText(e);
                eventsAnswer.value = null;
            }
            logger.error('Could not ask the timeline', e);
        });
    try {
        // Two questions about one query. The list shows the first page, ranked
        // and with snippets; the graph needs every id that matched, which is
        // why it cannot simply reuse the page the list is showing.
        const [response, matchIds] = await Promise.all([
            invoke<SearchResponse>('search_nexus', {
                vaultPath: props.vaultPath,
                query: searchQuery.value,
                caseSensitive: caseSensitive.value,
            }),
            invoke<string[]>('search_nexus_ids', {
                vaultPath: props.vaultPath,
                query: searchQuery.value,
                caseSensitive: caseSensitive.value,
            }),
        ]);
        if (searchId === currentSearchId) {
            searchResults.value = response.results;
            totalCount.value = response.total_count;
            queryTimeMs.value = response.query_time_ms;
            graphMatchIds.value = matchIds;
        }
    } catch (e) {
        if (searchId === currentSearchId) {
            searchRefused.value = refusalText(e);
            searchResults.value = [];
            totalCount.value = 0;
            graphMatchIds.value = [];
        }
        logger.error('Could not search', e);
    } finally {
        if (searchId === currentSearchId) {
            isSearching.value = false;
        }
    }
};

/** What `timeline::query` will return at most in one go (`AT_MOST`). */
const EVENTS_AT_MOST = 1000;

/**
 * A search starts where the person was standing, and ends there too.
 *
 * Asking something while looking at the timeline and landing on the node
 * tab — with the graph in the pane — threw away the one thing the screen
 * already knew: which half they were reading. So the first keystroke picks
 * the tab that matches the shape on screen, and clearing the box returns to
 * the shape that matches the tab they finished on.
 */
watch(searchQuery, (now, before) => {
    const asking = !!now.trim();
    const wasAsking = !!before?.trim();
    if (asking && !wasAsking) {
        searchTab.value = shownAs.value === 'timeline' ? 'events' : 'nodes';
    } else if (!asking && wasAsking) {
        void showAs(searchTab.value === 'events' ? 'timeline' : 'graph');
    }
});

watch(searchQuery, () => {
    timeRange.value = null;
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
        performSearch();
    }, 250);
});

watch(caseSensitive, () => {
    if (searchQuery.value.trim()) performSearch();
});

// Debounce wrapper: coalesces rapid-fire events (e.g. node:updated + vault:file-modified)
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
const debouncedLoad = (fn: () => void, ms = 300) => {
    if (_debounceTimer) clearTimeout(_debounceTimer);
    _debounceTimer = setTimeout(fn, ms);
};

/**
 * Re-read the vault, and re-run the query if one is open. The query has to go
 * with it: its results name nodes, and after a reload some of those nodes may
 * be gone and others may now qualify. The graph is drawn from that set, so a
 * stale one shows a picture of a vault that no longer exists.
 */
const reload = () => {
    loadAllData();
    if (searchQuery.value.trim()) performSearch();
};

onMounted(() => {
    // Pointer rather than mouse: a `PointerEvent` is a `MouseEvent`, so the
    // shared handlers read it unchanged, and the edge works under a finger.
    window.addEventListener('pointermove', answers.onMouseMove);
    window.addEventListener('pointerup', answers.onMouseUp);
    window.addEventListener('pointercancel', answers.onMouseUp);
    // A window that shrank must not leave the column wider than the room it
    // now has — including the width remembered from a larger screen.
    window.addEventListener('resize', answers.reclamp);
    answers.reclamp();
    loadAllData();
    loadProposals();
    bus.on('vault:file-modified', () => debouncedLoad(reload));
    bus.on('vault:file-created-deleted', () => debouncedLoad(reload));
    bus.on('vault:sync-completed', () => debouncedLoad(reload));

    // Cross-app subscribers: reload when nodes are mutated elsewhere
    bus.on('node:created', () => debouncedLoad(reload));
    bus.on('node:updated', () => debouncedLoad(reload));
    bus.on('node:deleted', () => debouncedLoad(reload));
});

onUnmounted(() => {
    window.removeEventListener('pointermove', answers.onMouseMove);
    window.removeEventListener('pointerup', answers.onMouseUp);
    window.removeEventListener('pointercancel', answers.onMouseUp);
    window.removeEventListener('resize', answers.reclamp);
});

const getTypeIcon = (type: string) => {
    if (type === 'note') return FileText;
    if (type === 'task') return CheckSquare;
    if (type === 'quickcap') return Zap;
    if (type === 'file') return File;
    if (type === 'event') return Calendar;
    if (type === 'tag') return Tag;
    if (type === 'whiteboard') return PenTool;
    if (type === 'person') return Users;
    if (type === 'decision') return Scale;
    return FileText;
};

const getTypeColor = (type: string) => {
    if (type === 'note') return 'text-blue-600 bg-blue-100 dark:bg-blue-500/20 dark:text-blue-400';
    if (type === 'task') return 'text-emerald-600 bg-emerald-100 dark:bg-emerald-500/20 dark:text-emerald-400';
    if (type === 'quickcap') return 'text-amber-600 bg-amber-100 dark:bg-amber-500/20 dark:text-amber-400';
    if (type === 'file') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    if (type === 'event') return 'text-rose-600 bg-rose-100 dark:bg-rose-500/20 dark:text-rose-400';
    if (type === 'tag') return 'text-purple-600 bg-purple-100 dark:bg-purple-500/20 dark:text-purple-400';
    if (type === 'whiteboard') return 'text-violet-600 bg-violet-100 dark:bg-violet-500/20 dark:text-violet-400';
    if (type === 'person') return 'text-orange-600 bg-orange-100 dark:bg-orange-500/20 dark:text-orange-400';
    if (type === 'decision') return 'text-amber-600 bg-amber-100 dark:bg-amber-500/20 dark:text-amber-400';
    return 'text-gray-600 bg-gray-100 dark:bg-gray-500/20 dark:text-gray-400';
};

const openPreview = async (item: NexusItem | SearchResult) => {
    emit('edit-item', item.id, item.item_type, searchQuery.value.trim() || undefined);
};

/**
 * Pressing something on the graph is asking a question about it.
 *
 * It fills the search box, which is now the only place a question lives. It
 * used to fill the query bar inside "Look back" — and since that bar only
 * existed while looking back, pressing a person or a tag anywhere else was a
 * **dead click**: the optional call did nothing and the `return` swallowed
 * the ordinary open as well.
 *
 * A person becomes their name rather than `with:`. `with:` is a word only
 * the timeline can answer, so it would refuse on the tab in front and answer
 * on the one behind — for a gesture the person did not type. Their name is a
 * question both halves can answer.
 */
const openPreviewFromGraph = (node: GraphNode) => {
    if (node.item_type === 'tag') {
        searchQuery.value = `#${node.title.replace(/^#/, '')}`;
        return;
    }
    if (node.item_type === 'person') {
        searchQuery.value = node.title;
        return;
    }
    emit('edit-item', node.id, node.item_type);
};

const closePreview = () => {
    selectedItem.value = null;
};

const renderMarkdownPreview = (text: string, type: string) => {
    if (!text) return '';
    
    let parsed = text;
    // Strip frontmatter if present (only for notes/tasks)
    if (type !== 'quickcap' && text.startsWith('---\n')) {
        const splitIdx = text.indexOf('---', 3);
        if (splitIdx > 0) {
            parsed = text.substring(splitIdx + 3).trim();
        }
    }
    
    // Convert relative asset links so they load properly in preview
    parsed = parsed.replace(/!\[.*?\]\((.*?)\)/g, (_match, path) => {
        let absPath = path;
        if (path.startsWith('assets/')) {
            absPath = `${props.vaultPath}/${path}`;
        }
        const src = convertFileSrc(absPath);
        return `![image](${src})`;
    });
    
    const html = marked.parse(parsed, { async: false, breaks: true }) as string;
    return DOMPurify.sanitize(html);
};

const cleanSnippet = (snippet: string) => {
    if (!snippet) return '';
    // Replace markdown images: ![alt](url) -> 🖼️ alt
    let text = snippet.replace(/!\[([^\]]*)\]\([^)]+\)/g, '🖼️ $1');
    // Replace HTML images
    text = text.replace(/<img[^>]*>/gi, '🖼️ Image');
    // Sanitize to only allow <mark> tags from FTS5
    return DOMPurify.sanitize(text, { ALLOWED_TAGS: ['mark'] });
};
</script>

<template>
  <div class="h-full w-full flex relative overflow-hidden bg-[#fdfdfc] dark:bg-[#1a1a1c] font-sans">
    
    <!-- Main UI -->
    <!-- `--answers` is the one place the column's width is written. The
         column, the pane beside it and the omnibar above it all read it, so
         they cannot drift apart, and `sm:` keeps the breakpoint in CSS. -->
    <div
        v-show="!selectedItem"
        ref="screen"
        class="flex-1 flex flex-col h-full relative transition-all"
        :style="{ '--answers': answers.leftWidth.value + 'px' }"
        :class="answers.isDraggingLeft.value ? 'resizing cursor-col-resize select-none' : ''"
    >
        
        <template v-if="currentView === 'graph_search'">
            <!-- Background Graph View -->
        <div
            class="absolute inset-y-0 right-0 z-0"
            :class="searchQuery ? 'left-0 sm:left-(--answers)' : 'left-0'"
        >
            <!-- One vault, two shapes: what is connected to what, and when.
                 Sits opposite the graph's own controls so neither hides the
                 other.

                 Only while browsing. With a question open the answer is read
                 in the tabs, and this drew the same rows a second time under
                 the same word. -->
            <div
                v-if="!searchQuery"
                data-shown-as
                class="absolute top-[100px] left-6 z-20 flex items-center gap-0.5 rounded-full border border-gray-200 bg-white/80 p-0.5 shadow-lg backdrop-blur-md dark:border-[#3a3a3c] dark:bg-[#242426]/80"
            >
                <button
                    v-for="option in (['graph', 'timeline'] as const)"
                    :key="option"
                    type="button"
                    :data-shape="option"
                    :aria-pressed="shownAs === option"
                    class="flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-semibold transition-colors"
                    :class="
                        shownAs === option
                            ? 'bg-indigo-600 text-white'
                            : 'text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-100'
                    "
                    @click="showAs(option)"
                >
                    <component :is="option === 'graph' ? Share2 : CalendarDays" class="h-3.5 w-3.5" />
                    {{ $t(`nexus.shown_as_${option}`) }}
                </button>
            </div>

            <!-- The vault's own timeline, for somebody browsing it with
                 nothing asked. -->
            <div
                v-if="paneShows === 'timeline'"
                data-timeline-pane
                class="absolute inset-0 overflow-y-auto bg-[#fdfdfc] pt-[150px] dark:bg-[#1a1a1c]"
            >
                <template v-if="wholeTimeline?.rows.length">
                    <!-- The picture first, then the rows it is a picture of;
                         a stretch picked on one narrows the other. -->
                    <div class="px-8 pb-3">
                        <TimeWindowPicker
                            :model-value="windowChoice"
                            :range="windowDays"
                            :hidden="windowHidden"
                            @update:model-value="pickWindow"
                        />
                    </div>
                    <div class="h-64 px-8 pb-4">
                        <EventsOverTime v-model="timeRange" :result="windowed" :span="windowDays" @open="openFromTimeline" />
                    </div>
                    <DatedView :result="within(windowed, timeRange)" @open="openFromTimeline" />
                </template>
                <p v-else data-timeline-empty class="px-8 text-[12px] text-gray-400">
                    {{ $t('nexus.lens_nothing') }}
                </p>
            </div>

            <!-- A question's events, drawn across time beside the list of
                 them. The whole pane, because here it is the picture. -->
            <div
                v-else-if="paneShows === 'events'"
                data-events-pane
                class="absolute inset-0 bg-[#fdfdfc] px-8 pb-24 pt-[120px] dark:bg-[#1a1a1c]"
            >
                <!-- The window row sits above what it scopes: this chart,
                     and the list in the tab beside it. -->
                <div v-if="eventsAnswer && !eventsRefused" class="flex h-full flex-col gap-4">
                    <TimeWindowPicker
                        :model-value="windowChoice"
                        :range="windowDays"
                        :hidden="windowHidden"
                        @update:model-value="pickWindow"
                    />
                    <div class="min-h-0 flex-1">
                        <EventsOverTime
                            v-model="timeRange"
                            :result="windowed"
                            :span="windowDays"
                            @open="openFromTimeline"
                        />
                    </div>
                </div>
                <p v-else class="text-[12px] text-gray-400">{{ eventsRefused ?? $t('nexus.lens_nothing') }}</p>
            </div>

            <GraphView
                v-else-if="graphData"
                :graph-data="graphData"
                :match-ids="graphMatchIds"
                @node-click="openPreviewFromGraph"
            />
            <div v-else class="w-full h-full flex items-center justify-center">
                <div class="w-8 h-8 rounded-full border-2 border-gray-300 dark:border-gray-600 border-t-transparent animate-spin"></div>
            </div>

            <!-- The two doors into the timeline, in one place.
                 One is the person saying what happened; the other is Syn
                 saying what it thinks happened and asking. They were at
                 opposite corners of the screen, so nothing suggested they
                 were two halves of the same thing — and every word on them
                 said "write" and "note", which is what they are **not**.
                 One frame, and both labels say *event*. -->
            <!-- No `overflow-hidden` on this frame: the compose form opens
                 upward out of it, and clipping that is how the button became
                 a button that did nothing. A smaller radius instead, so a
                 child's square hover still sits right inside it. -->
            <div
                v-if="!selectedItem"
                data-timeline-doors
                class="absolute bottom-6 left-6 z-20 flex items-stretch divide-x divide-gray-200 rounded-2xl border border-gray-200 bg-white/85 shadow-lg backdrop-blur-md dark:divide-[#3a3a3c] dark:border-[#3a3a3c] dark:bg-[#242426]/85"
            >
                <EventCompose
                    :vault-path="vaultPath"
                    :format="dailyNoteFormat"
                    :tag="dailyNoteTag"
                    @changed="eventsChanged"
                />
                <button
                    v-if="proposals?.waiting"
                    data-review-proposals
                    type="button"
                    class="flex items-center gap-2 px-4 py-2 text-xs font-semibold text-gray-700 transition-colors hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
                    :title="$t('nexus.review_title')"
                    @click="reviewing = true"
                >
                    <Sparkles class="h-4 w-4 text-indigo-500" />
                    {{ $t('nexus.review_title') }}
                    <span class="rounded-full bg-indigo-600 px-1.5 text-[10px] font-bold tabular-nums text-white">{{ proposals.waiting }}</span>
                </button>
            </div>
        </div>

        <!-- Header / Search OmniBar (Floating)
             Steps aside for the results column, the same way the pane does.
             It is `max-w-3xl mx-auto` inside a full-width box, so on a wide
             window it centred over the **whole** screen — which put it
             entirely to the right of the 480px column, while the column went
             on reserving 116px of blank for a bar that was no longer above
             it. On a phone the column is full width and the bar really is
             overhead, so the offset starts at `sm`, exactly where the column
             stops being full width. -->
        <div
            class="absolute top-0 right-0 pt-10 px-8 pb-6 z-20 pointer-events-none"
            :class="searchQuery ? 'left-0 sm:left-(--answers)' : 'left-0'"
        >
            <div class="max-w-3xl mx-auto flex items-center gap-6 pointer-events-auto">
                <NavButtons />
                <div class="flex-1 relative group">
                   <div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                       <Search class="h-5 w-5 text-gray-400 group-focus-within:text-black dark:group-focus-within:text-white transition-colors" />
                   </div>
                   <input 
                        v-model="searchQuery"
                        type="text" 
                        class="block w-full h-[52px] pl-12 pr-12 bg-white/80 dark:bg-[#242426]/80 border border-gray-200 dark:border-[#2c2c2e] rounded-2xl text-base text-black dark:text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-indigo-500/20 focus:border-indigo-500 dark:focus:border-indigo-500 shadow-lg backdrop-blur-xl transition-all"
                        placeholder="Universal Search... (e.g. is:task #urgent)"
                        @focus="showSyntaxHints = true"
                        @blur="hideSyntaxHints"
                        @keydown.esc="searchQuery = ''"
                    />
                   <div class="absolute inset-y-0 right-0 flex items-center gap-0.5 pr-3 pointer-events-auto">
                       <button
                           @click="caseSensitive = !caseSensitive"
                           :class="[
                               'w-7 h-7 flex items-center justify-center rounded-md text-xs font-bold font-mono transition-all',
                               caseSensitive
                                   ? 'bg-black text-white dark:bg-white dark:text-black shadow-sm'
                                   : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/10'
                           ]"
                           title="Case Sensitive"
                       >Aa</button>
                       <button v-if="searchQuery" @click="searchQuery = ''" class="w-7 h-7 flex items-center justify-center cursor-pointer" aria-label="Search Query =">
                           <X class="h-4 w-4 text-gray-400 hover:text-black dark:hover:text-white transition-colors" />
                       </button>
                   </div>

                   <!-- Search Syntax Hints Dropdown -->
                   <div v-if="showSyntaxHints && !searchQuery" class="absolute top-full left-0 right-0 mt-2 p-4 bg-white dark:bg-[#242426] border border-gray-200 dark:border-[#2c2c2e] rounded-xl shadow-xl z-50">
                       <p class="text-xs font-bold text-gray-500 dark:text-gray-400 mb-3 tracking-wider uppercase">Search Syntax</p>
                       <div class="grid grid-cols-2 gap-2 text-xs">
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">is:note</code><span class="text-gray-500">Filter by type</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">#tag</code><span class="text-gray-500">Filter by tag</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">"exact phrase"</code><span class="text-gray-500">Phrase match</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">-word</code><span class="text-gray-500">Exclude term</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">in:title</code><span class="text-gray-500">Title only</span></div>
                           <div class="flex items-center gap-2"><code class="px-1.5 py-0.5 bg-gray-100 dark:bg-[#1a1a1c] rounded font-mono text-indigo-600 dark:text-indigo-400">status:done</code><span class="text-gray-500">Task status</span></div>
                       </div>
                   </div>
                </div>

                <!-- Standalone Manage Tags Button -->
                <button 
                    @click="currentView = 'tag_manager'"
                    class="flex-shrink-0 w-12 h-[52px] flex items-center justify-center rounded-2xl bg-white/80 dark:bg-[#242426]/80 backdrop-blur-xl border border-gray-200 dark:border-[#2c2c2e] text-gray-500 dark:text-gray-400 hover:text-indigo-600 dark:hover:text-indigo-400 hover:border-indigo-300 dark:hover:border-[#444] shadow-lg transition-all active:scale-95 group"
                    title="Manage Tags"
                >
                    <Tag class="w-5 h-5 group-hover:scale-110 transition-transform" />
                </button>
            </div>
        </div>

        <!-- Search Results Column
             Full width on a phone, where there is no room for both; a column
             beside the graph everywhere else, so one query is answered as a
             list and as a picture at the same time. -->
        <div v-if="searchQuery" data-answers class="absolute inset-y-0 left-0 z-10 w-full sm:w-(--answers) sm:border-r border-gray-200 dark:border-[#2c2c2e] bg-[#fdfdfc]/95 dark:bg-[#1a1a1c]/95 backdrop-blur-xl flex flex-col animate-in fade-in slide-in-from-left-4 duration-200">
            <!-- The edge, draggable. A focusable `separator` is the ARIA
                 pattern for a window splitter, so the arrow keys move it for
                 anybody not using a pointer. Hidden below `sm`, where the
                 column is the whole screen and there is nothing to divide. -->
            <div
                data-answers-handle
                role="separator"
                aria-orientation="vertical"
                tabindex="0"
                :aria-label="$t('nexus.resize_answers')"
                :aria-valuenow="answers.leftWidth.value"
                :aria-valuemin="320"
                :aria-valuemax="900"
                class="group absolute inset-y-0 -right-1 z-20 hidden w-2 cursor-col-resize touch-none sm:block focus-visible:outline-none"
                @pointerdown.stop.prevent="beginResize($event)"
                @keydown.left.prevent="nudgeEdge(-24)"
                @keydown.right.prevent="nudgeEdge(24)"
                @dblclick="answers.leftWidth.value = 480"
            >
                <div
                    class="mx-auto h-full w-0.5 transition-colors group-hover:bg-indigo-400/60 group-focus-visible:bg-indigo-500"
                    :class="answers.isDraggingLeft.value ? 'bg-indigo-500' : ''"
                ></div>
            </div>
            <!-- Room for the omnibar, and only where the omnibar is.
                 Below `sm` it floats over this column and results would
                 scroll under it; from `sm` up it has stepped aside, so this
                 is a small gap rather than a band. The tab bar below carries
                 its own edge, so this one no longer draws a border. -->
            <div class="h-[116px] sm:h-4 flex-shrink-0 w-full bg-[#fdfdfc]/90 backdrop-blur-3xl sm:backdrop-blur-none dark:bg-[#1a1a1c]/90"></div>

            <!-- One question, both tables.
                 The count sits on the tab because that is the thing being
                 chosen between: "nothing here, plenty over there" is the
                 answer somebody needs before they press, not after. A tab
                 whose half refused carries no count — a refusal is not a
                 zero, and drawing it as one would say the timeline holds
                 nothing when it was never asked. -->
            <div
                data-search-tabs
                class="flex flex-shrink-0 items-center gap-1 border-b border-gray-200 px-4 pt-3 sm:px-6 dark:border-[#2c2c2e]"
            >
                <button
                    v-for="tab in (['nodes', 'events'] as const)"
                    :key="tab"
                    type="button"
                    data-search-tab
                    :data-tab="tab"
                    :aria-pressed="searchTab === tab"
                    class="-mb-px flex items-center gap-1.5 border-b-2 px-3 pb-2 text-[13px] font-semibold transition-colors"
                    :class="
                        searchTab === tab
                            ? 'border-indigo-600 text-indigo-600 dark:border-indigo-400 dark:text-indigo-400'
                            : 'border-transparent text-gray-400 hover:text-gray-700 dark:hover:text-gray-200'
                    "
                    @click="showTab(tab)"
                >
                    {{ $t(`nexus.search_tab_${tab}`) }}
                    <span
                        v-if="tab === 'nodes' ? !searchRefused : !eventsRefused"
                        data-tab-count
                        class="rounded-full bg-gray-100 px-1.5 text-[11px] font-bold tabular-nums text-gray-600 dark:bg-[#2c2c2e] dark:text-gray-300"
                    >{{ tab === 'nodes' ? totalCount : (eventsAnswer?.total ?? 0) }}</span>
                </button>
            </div>
            
            <div class="flex-1 overflow-y-auto px-4 sm:px-6 pb-16 pt-8">
                <div class="max-w-3xl mx-auto">
                <div v-if="isSearching" class="text-center py-10 opacity-50 flex items-center justify-center gap-2">
                    <div class="w-5 h-5 rounded-full border-2 border-black dark:border-white border-t-transparent animate-spin"></div>
                </div>
                
                <!-- ── what the vault holds ── -->
                <template v-else-if="searchTab === 'nodes'">
                    <!-- The engine says which half of the app can answer it;
                         showing that is the whole point of refusing rather
                         than quietly answering something else.

                         Ahead of the empty state, not inside the list below
                         it: a refused question leaves no results, so it used
                         to read «No results found» — the one sentence that
                         turns "I cannot ask that" into "there is nothing
                         there". -->
                    <p
                        v-if="searchRefused"
                        data-search-refused
                        class="rounded-2xl border border-amber-200 bg-amber-50 p-4 text-[13px] leading-relaxed text-amber-900 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-200"
                    >
                        {{ searchRefused }}
                    </p>

                    <div v-else-if="searchResults.length === 0" class="text-center py-16">
                        <div class="w-20 h-20 bg-gray-50 dark:bg-white/5 rounded-full flex flex-col items-center justify-center mx-auto mb-4 border border-dashed border-gray-200 dark:border-white/10">
                            <Search class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                        </div>
                        <p class="text-[#52525b] dark:text-[#a1a1aa] font-medium">No results found for "{{ searchQuery }}"</p>
                    </div>

                <div v-else class="space-y-3">
                    <div class="flex items-center justify-between mb-4">
                        <h3 class="text-sm font-bold text-gray-500 dark:text-gray-400">Search Results</h3>
                        <div class="flex items-center gap-3">
                            <span class="text-xs font-mono text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-200 dark:border-emerald-500/20">{{ queryTimeMs }}ms</span>
                            <span class="text-xs font-semibold text-gray-400">{{ totalCount }} items</span>
                        </div>
                    </div>

                    <div v-for="item in searchResults" :key="item.id"
                         @click="openPreview(item)"
                         class="group flex gap-4 p-4 rounded-2xl bg-white dark:bg-[#242426] border border-gray-100 dark:border-[#2c2c2e] hover:border-indigo-300 dark:hover:border-indigo-500/50 shadow-sm hover:shadow-md cursor-pointer transition-all active:scale-[0.99]"
                    >
                        <!-- Icon Badge -->
                        <div class="flex-shrink-0 mt-1">
                            <div class="w-10 h-10 rounded-xl flex items-center justify-center shadow-inner" :class="getTypeColor(item.item_type)">
                                <component :is="getTypeIcon(item.item_type)" class="w-5 h-5 stroke-[1.5]" />
                            </div>
                        </div>

                        <!-- Content -->
                        <div class="flex-1 min-w-0 flex flex-col justify-center">
                            <!-- Wraps rather than squeezes. With the column
                                 dragged narrow, the date badge kept its
                                 width and the title was truncated to two
                                 letters — «Mộ…», «Tôi …». Now the badge
                                 drops under the title when there is not
                                 room for both, and the title keeps its
                                 words. -->
                            <div class="flex flex-wrap items-start justify-between gap-x-4 gap-y-1 mb-1">
                                <h4 data-hit-title class="min-w-[8rem] flex-1 break-words line-clamp-2 font-bold text-[15px] text-[#1c1c1e] dark:text-[#f4f4f5] group-hover:text-indigo-600 dark:group-hover:text-indigo-400 transition-colors">{{ item.title }}</h4>
                                <!-- `date` is the node's `updated_at`, which
                                     on a daily note is a different day from
                                     its own title. Unlabelled, one card
                                     carried two dates and the louder of them
                                     was the one nobody meant. -->
                                <span class="flex-shrink-0 text-[10px] font-bold text-gray-400 flex items-center gap-1 bg-gray-50 dark:bg-[#1a1a1c] px-2 py-0.5 rounded-md border border-gray-100 dark:border-[#2c2c2e]">
                                    <span class="font-medium opacity-60">{{ $t('nexus.hit_edited') }}</span>{{ localDay(item.date) }}
                                </span>
                            </div>
                            
                            <p v-if="appLockStore.isEnabled && appLockStore.isNoteProtected(item.id)" class="text-[13px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 leading-relaxed flex items-center gap-1.5">
                              <Lock class="w-3.5 h-3.5 text-amber-500 shrink-0" />
                              <span class="italic text-gray-400 dark:text-gray-500">Content protected</span>
                            </p>
                            <p v-else-if="item.item_type !== 'file'" class="text-[13px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 leading-relaxed preview-markdown break-words" v-html="cleanSnippet(item.snippet)"></p>
                            <p v-else class="text-[13px] text-purple-600/70 dark:text-purple-400/70 font-mono break-words" v-html="cleanSnippet(item.snippet)"></p>
                            
                            <div class="flex items-center gap-2 mt-3" v-if="item.tags.length > 0">
                                <span v-for="tag in item.tags" :key="tag" class="text-[10px] font-bold px-2 py-0.5 rounded bg-gray-100 dark:bg-[#1a1a1c] border border-gray-200 dark:border-[#2c2c2e] text-gray-600 dark:text-gray-400 flex items-center gap-1">
                                    <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>
                </template>

                <!-- ── what happened ──
                     The same words asked of `events`, drawn down the days.
                     `DatedView` was written for notes long before the
                     timeline had a language; it needs no telling what an
                     event is, because both halves come back as one
                     `QueryResult`. -->
                <template v-else>
                    <p
                        v-if="eventsRefused"
                        data-events-refused
                        class="rounded-2xl border border-amber-200 bg-amber-50 p-4 text-[13px] leading-relaxed text-amber-900 dark:border-amber-500/20 dark:bg-amber-500/10 dark:text-amber-200"
                    >
                        {{ eventsRefused }}
                    </p>

                    <div v-else-if="!eventsAnswer?.rows.length" data-events-empty class="text-center py-16">
                        <div class="w-20 h-20 bg-gray-50 dark:bg-white/5 rounded-full flex flex-col items-center justify-center mx-auto mb-4 border border-dashed border-gray-200 dark:border-white/10">
                            <CalendarDays class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                        </div>
                        <p class="text-[#52525b] dark:text-[#a1a1aa] font-medium">{{ $t('nexus.lens_nothing') }}</p>
                    </div>

                    <div v-else data-events-results>
                        <div class="flex items-center justify-between mb-4">
                            <h3 class="text-sm font-bold text-gray-500 dark:text-gray-400">{{ $t('nexus.search_tab_events') }}</h3>
                            <span class="text-xs font-mono text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-500/10 px-2 py-0.5 rounded-md border border-emerald-200 dark:border-emerald-500/20">{{ eventsAnswer.query_time_ms }}ms</span>
                        </div>

                        <!-- What this answer could not have known about. -->
                        <p
                            v-if="proposals && (proposals.waiting || proposals.unread)"
                            data-events-backlog
                            class="mb-3 text-[11px] leading-relaxed text-gray-500 dark:text-gray-400"
                        >
                            {{ proposals.waiting
                                ? $t('nexus.events_partial_proposals', { count: proposals.waiting }, proposals.waiting)
                                : $t('nexus.events_partial_unread', { count: proposals.unread }, proposals.unread) }}
                            <!-- Seeing the answer is short and being able to
                                 do something about it belong together. -->
                            <button
                                v-if="proposals.waiting"
                                type="button"
                                data-review-from-timeline
                                class="ml-1 font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
                                @click="reviewing = true"
                            >{{ $t('nexus.review_open') }}</button>
                        </p>
                        <!-- Narrowed by whatever stretch is picked on the
                             chart; the count up top stays the whole answer. -->
                        <p v-if="timeRange" data-events-narrowed class="mb-3 text-[12px] text-gray-500 dark:text-gray-400">
                            {{ $t('nexus.events_narrowed', { from: timeRange.from, to: timeRange.to }) }}
                            <button
                                type="button"
                                class="ml-1 font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
                                @click="timeRange = null"
                            >{{ $t('nexus.over_time_clear') }}</button>
                        </p>
                        <!-- The window scopes this list too, and says what
                             it leaves out: a list shorter than its tab's
                             count, with no reason given, reads as missing. -->
                        <p v-if="windowHidden" data-events-windowed class="mb-3 text-[12px] text-gray-500 dark:text-gray-400">
                            {{ $t('nexus.window_hidden', { n: windowHidden }, windowHidden) }}
                            <button
                                type="button"
                                class="ml-1 font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
                                @click="pickWindow('all')"
                            >{{ $t('nexus.window_show_all') }}</button>
                        </p>
                        <DatedView :result="within(windowed, timeRange)" @open="openFromTimeline" />
                    </div>
                </template>
            </div>
            </div>
        </div>
        </template>

        <!-- Main UI: Tag Management -->
        <NexusTagManager 
            v-else-if="currentView === 'tag_manager'" 
            :vaultPath="vaultPath" 
            @back="currentView = 'graph_search'" 
            @search-tag="(tag) => { searchQuery = `tag:&#34;${tag}&#34;`; currentView = 'graph_search'; }"
        />
    </div>

    <!-- Reviewing what Syn read out of the notes.
         The whole window, not a popover: every row is a sentence, a day, a
         list of people and the line it was read from, and there can be
         dozens of them. The settings sit under the queue rather than over
         it — the queue is what somebody came for. -->
    <div
        v-if="reviewing"
        data-review-screen
        class="absolute inset-0 z-40 flex flex-col bg-[#fdfdfc] dark:bg-[#1a1a1c] animate-in fade-in duration-150"
    >
        <div class="h-16 flex-shrink-0 flex items-center justify-between border-b border-gray-200 bg-white/80 px-6 backdrop-blur-md dark:border-[#2c2c2e] dark:bg-[#242426]/80">
            <div class="flex items-center gap-3">
                <button
                    data-review-close
                    class="group -ml-2 flex items-center gap-1 rounded-xl p-2 text-gray-500 transition-colors hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
                    @click="reviewing = false"
                >
                    <ChevronRight class="h-5 w-5 rotate-180 transition-transform group-hover:-translate-x-0.5" />
                    <span class="text-sm font-semibold">{{ $t('nexus.review_back') }}</span>
                </button>
                <div class="h-4 w-px bg-gray-300 dark:bg-[#444]"></div>
                <h2 class="text-sm font-bold text-gray-800 dark:text-gray-200">{{ $t('nexus.review_title') }}</h2>
            </div>
            <span v-if="proposals?.waiting" class="text-xs font-semibold tabular-nums text-gray-400">
                {{ $t('nexus.review_waiting', { count: proposals.waiting }) }}
            </span>
        </div>

        <div class="flex-1 overflow-y-auto px-6 py-8 sm:px-10">
            <div class="mx-auto max-w-6xl">
                <ExtractTray
                    :vault-path="vaultPath"
                    @changed="eventsChanged"
                    @open="(id: string, type: string, quote: string) => { reviewing = false; emit('edit-item', id, type, quote); }"
                />
            </div>
        </div>
    </div>

    <!-- Full-page Preview Panel (Unchanged logic, floating on top when active) -->
    <div v-if="selectedItem" class="absolute inset-0 bg-[#fdfdfc] dark:bg-[#1a1a1c] flex flex-col z-30 animate-in fade-in zoom-in-95 duration-200">
        <!-- Header -->
        <div class="h-16 border-b border-gray-200 dark:border-[#2c2c2e] flex items-center justify-between px-6 flex-shrink-0 bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md">
            <div class="flex items-center gap-4">
                <button @click="closePreview" class="p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-[#3a3a3c] text-gray-500 transition-colors flex items-center gap-1 group">
                    <ChevronRight class="w-5 h-5 rotate-180 transition-transform group-hover:-translate-x-0.5" /> <span class="text-sm font-semibold">Back</span>
                </button>
                <div class="h-4 w-px bg-gray-300 dark:bg-[#444]"></div>
                <div class="flex items-center gap-2">
                    <div class="p-1.5 rounded-lg" :class="getTypeColor(selectedItem.item_type)">
                        <component :is="getTypeIcon(selectedItem.item_type)" class="w-4 h-4" />
                    </div>
                    <span class="text-xs font-bold tracking-widest text-gray-800 dark:text-gray-200 uppercase">{{ selectedItem.item_type }}</span>
                </div>
            </div>
            
            <button @click="emit('edit-item', selectedItem.id, selectedItem.item_type)" class="px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-sm font-bold transition-all active:scale-95 flex items-center gap-2 shadow-sm">
                <component :is="getTypeIcon(selectedItem.item_type)" class="w-4 h-4" /> Edit Source
            </button>
        </div>

        <!-- Content Area -->
        <div class="flex-1 overflow-y-auto px-8 sm:px-16 md:px-32 py-12">
            <div class="max-w-4xl mx-auto">
                <h2 class="text-4xl font-extrabold text-[#1c1c1e] dark:text-white mb-6 leading-tight tracking-tight">{{ selectedItem.title }}</h2>
                
                <div class="flex flex-wrap gap-2 mb-10" v-if="selectedItem.tags.length">
                    <span v-for="tag in selectedItem.tags" :key="tag" class="text-xs font-medium px-2.5 py-1 rounded bg-gray-100 dark:bg-[#2c2c2e] text-gray-700 dark:text-gray-300 flex items-center gap-1 border border-gray-200 dark:border-[#3a3a3c]">
                        <span class="opacity-50">#</span>{{ tag.split('/').pop() }}
                    </span>
                </div>

                <div class="prose prose-lg dark:prose-invert prose-zinc max-w-none leading-loose preview-markdown" v-html="renderMarkdownPreview(selectedItem.content, selectedItem.item_type)">
                </div>
                
                <div class="mt-16 p-4 bg-gray-50 dark:bg-[#242426] rounded-xl border border-gray-200 dark:border-[#2c2c2e]">
                    <div class="text-xs font-medium text-gray-500 flex justify-between items-center">
                        <div class="flex-1 min-w-0">
                            <span class="block opacity-70 mb-1">Source Path:</span>
                            <code class="block truncate text-gray-800 dark:text-gray-300">{{ selectedItem.path }}</code>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>


  </div>
</template>

<style scoped>
/*
 * Nothing re-blurs while the split is being dragged.
 *
 * The answers column, the omnibar and the band under it are all
 * `backdrop-blur`, and a backdrop filter is re-computed over its whole area
 * every time that area changes — which, during a drag, is every frame. Notes
 * feels smooth partly because its sidebar is a flat colour.
 *
 * Switching it off costs nothing to look at: these surfaces sit on
 * `bg-…/95`, so the blur is doing almost no work even when it is on, and
 * nobody is reading through them while their hand is on the edge.
 */
.resizing :deep([class*='backdrop-blur']) {
    backdrop-filter: none;
    -webkit-backdrop-filter: none;
}

.preview-markdown :deep(img) {
    display: inline-block;
    max-height: 120px;
    margin: 8px 0;
    border-radius: 8px;
    box-shadow: 0 4px 6px -1px rgb(0 0 0 / 0.1);
}
.preview-markdown :deep(input[type="checkbox"]) {
    margin-right: 6px;
    accent-color: #10b981;
}
/* FTS5 search highlight */
.preview-markdown :deep(mark) {
    background: rgba(250, 204, 21, 0.3);
    color: inherit;
    border-radius: 2px;
    padding: 0 2px;
}
@media (prefers-color-scheme: dark) {
    .preview-markdown :deep(mark) {
        background: rgba(250, 204, 21, 0.2);
    }
}
</style>
