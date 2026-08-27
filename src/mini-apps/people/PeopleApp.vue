<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch, nextTick } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { ask } from '@tauri-apps/plugin-dialog';
import { Users, Plus, Mail, Phone, Building, Hash, Search, Edit2, Gift, Briefcase, LayoutDashboard, Clock, FileText, Share2, ArrowUpDown, AlertCircle, CalendarPlus, UserPlus, Upload, Download } from 'lucide-vue-next';
import PersonModal from './PersonModal.vue';
import GiftModal from './GiftModal.vue';
import OverviewTab from './OverviewTab.vue';
import NotesTab from './NotesTab.vue';
import TimelineTab from './TimelineTab.vue';
import GraphTab from './GraphTab.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import RemindersWidget from './RemindersWidget.vue';
import LinkPersonModal from './LinkPersonModal.vue';
import PeopleManager from './PeopleManager.vue';
import ImportContactsModal from './ImportContactsModal.vue';

import { contactPercent, contactDotClass, contactStatus } from './composables/useRelationshipHealth';
import { linkRemovalPatches, namesFor, pointsAt, type Connection } from './composables/connections';
import { parseAnnualDate } from './composables/anniversaries';
import { relationshipsOf, relationshipLabel } from './composables/relationships';
import { searchPeople } from './composables/search';
import { segmentFromNode, segmentToProperties, peopleIn, type Segment } from './composables/segments';
import { useListKeyboard } from './composables/useListKeyboard';
import SegmentModal from './SegmentModal.vue';
import { useContactExchange } from './composables/useContactExchange';
import { logger } from '../../utils/logger';

const bus = useEventBus();
const ns = useNodeService();

// Helper: get first detail value by label keyword
const getPersonDetail = (person: any, keyword: string): string => {
    const d = person?.properties?.details?.find((d: any) => d.label.toLowerCase().includes(keyword));
    return d?.value || person?.properties?.[keyword] || '';
};

const getDisplayName = (person: any): string => {
    const p = person?.properties;
    if (p?.display_name === 'nickname' && p?.nickname) return p.nickname;
    if (p?.display_name === 'custom' && p?.custom_display) return p.custom_display;
    return person.title;
};

const props = defineProps<{
    vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

const route = useRoute();
const router = useRouter();

const people = ref<any[]>([]);
const searchQuery = ref('');
const loading = ref(true);
const showModal = ref(false);
const selectedPerson = ref<any | null>(null);
const activeTab = ref<'overview' | 'timeline' | 'notes' | 'graph'>('overview');
const sortMode = ref<'alpha' | 'recent' | 'attention'>('recent');
const showGiftModal = ref(false);

// Linked nodes for Notes/Timeline tabs
const linkedNodes = ref<any[]>([]);
const loadingLinks = ref(false);

const allDebts = ref<any[]>([]);
const allTransactions = ref<any[]>([]);

const isMobile = ref(window.innerWidth < 768);
const handleResize = () => { isMobile.value = window.innerWidth < 768; };
const isSidebarOpen = ref(false);

const exchange = useContactExchange(ns, () => props.vaultPath);
const showImportModal = ref(false);

/**
 * Write the whole address book to a file.
 *
 * vCard, because that is what another contact app will read. The spreadsheet
 * is offered from the same place for anybody taking the list somewhere that
 * is not a contact app at all.
 */
const exportAll = async () => {
    try {
        await exchange.exportContacts('vcard');
    } catch (e) {
        logger.error('Failed to export contacts', e);
    }
};

/**
 * The list, without the bodies.
 *
 * `getNodes` sends every person's body as well, and the list renders none of
 * it — only the one person who is open needs a body, and `selectPerson`
 * fetches that one. On a vault of a few thousand contacts the difference is
 * most of the payload.
 */
const fetchPeople = async () => {
    loading.value = true;
    try {
        const [summaries, lastSeen] = await Promise.all([
            ns.getNodeSummaries('person'),
            // A vault with no links yet answers with nothing, and an older
            // build answers with nothing at all. Neither is a reason for the
            // list of people to fail to load.
            invoke<Record<string, string>>('last_contact_dates').catch(() => ({})),
        ]);
        people.value = summaries.map((person: any) => withDerivedContact(person, lastSeen));
        if (selectedPerson.value) {
            // Re-read the open person in full: the summary in the list has no
            // body, and the Notes tab shows it.
            selectedPerson.value = await ns.getNode(selectedPerson.value.id);
        }
        // Ensure owner person exists
        await ensureOwner();
    } catch (e) {
        logger.error('Failed to fetch people nodes', e);
    } finally {
        loading.value = false;
    }
};

/**
 * Fold in when the vault last saw this person, if that is later.
 *
 * A note that mentions somebody is a touch, and the reminder engine already
 * counts it. Doing the same here is what keeps the dot beside their name and
 * the notification about them telling the same story. Nothing is written: the
 * database works this out from links that already exist, and storing it would
 * mean a write and a sync round every time a note is saved.
 */
const withDerivedContact = (person: any, lastSeen: Record<string, string> | null | undefined) => {
    const seen = lastSeen?.[person.id];
    if (!seen) return person;
    const stored = person.properties?.last_contacted ?? '';
    // Both are `YYYY-MM-DD`, so comparing as text compares as dates.
    if (seen <= stored) return person;
    return { ...person, properties: { ...person.properties, last_contacted: seen } };
};

/** Open a person, fetching the body the list does not carry. */
const selectPerson = async (person: any) => {
    if (!person) { selectedPerson.value = null; return; }
    try {
        selectedPerson.value = await ns.getNode(person.id) || person;
    } catch (e) {
        logger.error('Failed to load person', e);
        selectedPerson.value = person;
    }
};

const ensureOwner = async () => {
    const hasOwner = people.value.some(p => p.properties?.is_owner === true);
    if (hasOwner) return;
    try {
        const relPath = `People/owner.md`;
        await ns.writeNode({
            relPath, title: 'Me',
            nodeType: 'person',
            properties: { is_owner: true, tags: ['owner'] },
            content: '',
            eventType: 'created',
            silent: true,
        });
        people.value = await ns.getNodeSummaries('person');
    } catch (e) {
        logger.error('Failed to create owner person', e);
    }
};

const fetchLinkedNodes = async (personTitle: string, personId: string) => {
    loadingLinks.value = true;
    try {
        const links = await ns.getLinkedNodes(personTitle, personId);
        linkedNodes.value = links;
    } catch (e) {
        logger.error('Failed to fetch linked nodes', e);
        linkedNodes.value = [];
    } finally {
        loadingLinks.value = false;
    }
};

/**
 * Money owed and money moved, loaded only once the Timeline is looked at.
 *
 * These read every month the vault has ever recorded, transactions and all,
 * to pull out the handful belonging to one person. Doing that on the way into
 * People made opening a contact list pay for the whole finance history —
 * before anything had asked to see it.
 */
let financeLoaded = false;

const loadFinance = async (force = false) => {
    if (financeLoaded && !force) return;
    financeLoaded = true;
    await Promise.all([fetchDebts(), fetchTransactions()]);
};

const fetchDebts = async () => {
    try {
        const debtNodes = await ns.getNodes('finance_debts');
        if (debtNodes.length > 0 && debtNodes[0].properties && debtNodes[0].properties.debts) {
            allDebts.value = debtNodes[0].properties.debts;
        } else {
            allDebts.value = [];
        }
    } catch (e) {
        logger.error('Failed to fetch finance debts', e);
        allDebts.value = [];
    }
};

const fetchTransactions = async () => {
    try {
        const monthNodes = await ns.getNodes('finance_month');
        const flat: any[] = [];
        for (const node of monthNodes) {
            if (node.properties && node.properties.transactions) {
                flat.push(...node.properties.transactions);
            }
        }
        allTransactions.value = flat.sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());
    } catch (e) {
        logger.error('Failed to fetch finance transactions', e);
        allTransactions.value = [];
    }
};

watch(activeTab, (tab) => {
    if (tab === 'timeline') loadFinance();
});

watch(() => selectedPerson.value?.id, (newId, oldId) => {
    if (newId !== oldId) {
        if (selectedPerson.value && selectedPerson.value.title) {
            fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
        } else {
            linkedNodes.value = [];
        }
        activeTab.value = 'overview';
        if (isMobile.value) isSidebarOpen.value = false;
    }
});

// Debounce wrapper: coalesces rapid-fire events (e.g. node:updated + vault:file-modified)
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
const debouncedLoad = (fn: () => void, ms = 300) => {
    if (_debounceTimer) clearTimeout(_debounceTimer);
    _debounceTimer = setTimeout(fn, ms);
};

const debouncedRefreshAll = () => {
    debouncedLoad(() => {
        fetchPeople();
        if (financeLoaded) loadFinance(true);
        if (selectedPerson.value) fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
    });
};

onMounted(async () => {
    // Move anything still kept the old way before the list is drawn from it.
    // Safe to run every time: a vault already in the new shape produces an
    // empty plan and no file is touched.
    try {
        const moved = await invoke<{ interactions_moved: number }>('migrate_people_storage', {
            vaultPath: props.vaultPath,
        });
        if (moved.interactions_moved > 0) {
            logger.info(`Moved ${moved.interactions_moved} interactions into files of their own`);
        }
    } catch (e) {
        // A vault that could not be tidied still works; it just keeps the old
        // shape for now.
        logger.error('Could not tidy people storage', e);
    }

    await fetchPeople();
    fetchSegments();
    
    // Check URL for direct person link
    if (route.query.id) {
        openPersonById(route.query.id as string);
        // Clear query to avoid re-triggering later unexpectedly, or keep it.
        router.replace({ query: {} });
    }

    window.addEventListener('resize', handleResize);

    bus.on('vault:file-created-deleted', () => {
        debouncedLoad(() => {
            fetchPeople();
            if (selectedPerson.value) fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
        });
    });
    bus.on('vault:file-modified', () => {
        debouncedRefreshAll();
    });

    bus.on('vault:sync-completed', () => {
        debouncedLoad(() => fetchPeople());
    });

    // Cross-app: refresh when person nodes change from other apps
    bus.on('node:created', ({ nodeType }) => {
        if (nodeType === 'person') debouncedLoad(() => fetchPeople());
        if (nodeType === 'finance_month' || nodeType === 'finance_debts') {
            if (financeLoaded) debouncedLoad(() => loadFinance(true));
        }
    });

    bus.on('node:updated', ({ nodeType }) => {
        if (nodeType === 'person') {
            debouncedLoad(() => {
                fetchPeople();
                if (selectedPerson.value) fetchLinkedNodes(selectedPerson.value.title, selectedPerson.value.id);
            });
        }
        if (nodeType === 'finance_month' || nodeType === 'finance_debts') {
            if (financeLoaded) debouncedLoad(() => loadFinance(true));
        }
    });

    bus.on('node:deleted', ({ nodeType }) => {
        if (nodeType === 'person') debouncedLoad(() => fetchPeople());
    });
});

onUnmounted(() => {
    window.removeEventListener('resize', handleResize);
    // A refresh queued on the way out would run against a screen that is no
    // longer there. Event-bus subscriptions clean themselves up; these two
    // never did.
    if (_debounceTimer) clearTimeout(_debounceTimer);
});

// ─── Saved segments ─────────────────────────────────────────
//
// A saved question, not a saved list: a list would be wrong the moment
// somebody new answered it. Kept as `filter` nodes, which the app already
// has, so a segment syncs and is searchable like anything else.
const segments = ref<Segment[]>([]);
const activeSegmentId = ref<string | null>(null);
const showSegmentModal = ref(false);
const editingSegment = ref<Segment | null>(null);

const activeSegment = computed(() =>
    segments.value.find(s => s.id === activeSegmentId.value) ?? null);

const fetchSegments = async () => {
    try {
        const nodes = await ns.getNodeSummaries('filter');
        segments.value = nodes
            .filter((n: any) => n.properties?.subject === 'person')
            .map(segmentFromNode);
    } catch (e) {
        logger.error('Failed to load segments', e);
        segments.value = [];
    }
};

const saveSegment = async (draft: Omit<Segment, 'id'>, existingId?: string) => {
    const name = draft.name.trim();
    if (!name) return;
    try {
        const relPath = existingId || `Filters/people-${crypto.randomUUID()}.md`;
        await ns.writeNode({
            relPath,
            title: name,
            nodeType: 'filter',
            properties: segmentToProperties(draft),
            eventType: existingId ? 'updated' : 'created',
        });
        await fetchSegments();
        activeSegmentId.value = relPath;
        showSegmentModal.value = false;
    } catch (e) {
        logger.error('Failed to save the segment', e);
    }
};

const deleteSegment = async (id: string) => {
    try {
        await ns.deleteNode({ relPath: id });
        if (activeSegmentId.value === id) activeSegmentId.value = null;
        await fetchSegments();
    } catch (e) {
        logger.error('Failed to delete the segment', e);
    }
};

const filteredPeople = computed(() => {
    let list = people.value.filter(p => !p.properties?.is_owner);
    if (activeSegment.value) list = peopleIn(activeSegment.value, list);
    list = searchPeople(list, searchQuery.value);
    // Sort
    if (sortMode.value === 'alpha') {
        list = [...list].sort((a, b) => a.title.localeCompare(b.title));
    } else if (sortMode.value === 'recent') {
        list = [...list].sort((a, b) => {
            const aTime = a.updated_at ? new Date(a.updated_at).getTime() : (a.created_at ? new Date(a.created_at).getTime() : 0);
            const bTime = b.updated_at ? new Date(b.updated_at).getTime() : (b.created_at ? new Date(b.created_at).getTime() : 0);
            return bTime - aTime;
        });
    } else if (sortMode.value === 'attention') {
        list = [...list].sort((a, b) => {
            const aScore = getHealthScore(a);
            const bScore = getHealthScore(b);
            return aScore - bScore;
        });
    }
    return list;
});

const sidebarPeople = computed(() => {
    return filteredPeople.value.slice(0, 20);
});

// Arrow keys move through the list; Enter opens whoever is on. Focus follows,
// so the browser does the scrolling rather than a hand-rolled one fighting it.
const activeRow = ref<HTMLElement | null>(null);
const listKeys = useListKeyboard(
    computed(() => sidebarPeople.value),
    person => selectPerson(person)
);
watch(listKeys.activeIndex, async () => {
    await nextTick();
    activeRow.value?.focus();
});

const needsAttentionCount = computed(() => {
    // Asking the status directly rather than reading it back out of a colour
    // class — the colour is how it is drawn, not what it means.
    return people.value.filter(p => {
        const status = contactStatus(p);
        return status === 'overdue' || status === 'due_soon';
    }).length;
});

const topRelationships = computed(() => {
    const counts: Record<string, number> = {};
    people.value.forEach(p => {
        for (const relationship of relationshipsOf(p)) {
            const key = relationship.toLowerCase();
            counts[key] = (counts[key] || 0) + 1;
        }
    });
    return Object.entries(counts)
        .sort((a, b) => b[1] - a[1]) // Sort by frequency descending
        .slice(0, 10) // Top 10
        .map(([r]) => r.split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '));
});

const allTags = computed(() => {
    const tags = new Set<string>();
    for (const person of people.value) {
        for (const tag of person.properties?.tags ?? []) {
            if (typeof tag === 'string' && tag.trim()) tags.add(tag.toLowerCase());
        }
    }
    return Array.from(tags).sort();
});

const allRelationships = computed(() => {
    const rels = new Set<string>();
    people.value.forEach(p => {
        for (const relationship of relationshipsOf(p)) {
            rels.add(relationship.toLowerCase());
        }
    });
    return Array.from(rels)
        .filter(r => r)
        .map(r => r.split(' ').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '))
        .sort();
});

const cycleSortMode = () => {
    const modes: Array<'alpha' | 'recent' | 'attention'> = ['alpha', 'recent', 'attention'];
    const idx = modes.indexOf(sortMode.value);
    sortMode.value = modes[(idx + 1) % modes.length];
};

const sortLabel = computed(() => {
    const labels: Record<string, string> = { alpha: 'A-Z', recent: 'Recent', attention: 'Needs Attention' };
    return labels[sortMode.value];
});

const openNewModal = () => {
    selectedPerson.value = null;
    showModal.value = true;
};

const editPerson = async (person: any) => {
    await selectPerson(person);
    showModal.value = true;
};

const getInitials = (name: string) => {
    if (!name) return '?';
    return name.split(' ').map(n => n[0]).join('').substring(0, 2).toUpperCase();
};

const getTagColor = (tag: string) => {
    const colors = [
        'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300',
        'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300',
        'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-300',
        'bg-orange-100 text-orange-800 dark:bg-orange-900/30 dark:text-orange-300',
        'bg-pink-100 text-pink-800 dark:bg-pink-900/30 dark:text-pink-300',
    ];
    let hash = 0;
    for (let i = 0; i < tag.length; i++) hash = tag.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
};

const openLinkedNode = (node: any) => emit('open-node', node.id, node.node_type);

const getAvatarSrc = (person: any) => {
    if (!person?.properties?.avatar) return '';
    return convertFileSrc(`${props.vaultPath}/${person.properties.avatar}`);
};

// Both of these come from the shared cadence table, so the sidebar dot, the
// reminders widget and the person's own card cannot disagree about a status.
const getHealthScore = contactPercent;
const getContactHealthDot = contactDotClass;

const tabs = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'timeline', label: 'Timeline', icon: Clock },
    { id: 'notes', label: 'Notes & Links', icon: FileText },
    { id: 'graph', label: 'Graph', icon: Share2 },
];

const handleTimelineUpdated = () => {
    fetchPeople();
};

const handleGiftSaved = async (gift: any) => {
    if (!selectedPerson.value) return;
    try {
        // Re-read before appending. This screen's copy of the person is as old
        // as the last refresh, and spreading it into the write sent every
        // other field back as it stood then — the last place in People still
        // doing that, and a way to lose an interaction logged in another
        // window between one gift and the next.
        const current = await ns.getNode(selectedPerson.value.id);
        const gifts = [gift, ...(current?.properties?.gifts || [])];
        await ns.writeNode({
            relPath: selectedPerson.value.id,
            title: current?.title || selectedPerson.value.title,
            nodeType: 'person',
            properties: { gifts },
        });
        fetchPeople();
    } catch (e) {
        logger.error('Failed to save gift', e);
    }
};

const openPersonById = async (id: string) => {
    if (people.value.length === 0) await fetchPeople();
    let p = people.value.find(p => p.id === id);
    if (!p) p = people.value.find(p => p.title.toLowerCase() === id.toLowerCase());
    if (!p) {
        const slug = id.replace(/[^a-z0-9]/gi, '_').toLowerCase();
        if (slug.length > 2) p = people.value.find(p => p.id.toLowerCase().includes(slug));
    }
    if (p) await selectPerson(p);
};

/**
 * Put every birthday on the calendar, as one repeating entry each.
 *
 * It used to write an event per person per year, named after the year. That
 * meant the calendar was right until the 31st of December and then silently
 * empty until somebody pressed the button again, and it left a trail of dead
 * events behind for anybody deleted in the meantime.
 *
 * One yearly rule instead, on a path derived from the person rather than from
 * the date, so pressing this twice updates rather than duplicates. The
 * recurrence engine already knows what to do with the 29th of February.
 *
 * These entries are for *looking at*. The announcement comes from the person
 * themselves — see `plan_birthday` — and `source_person_id` is what tells the
 * reminder engine not to say it twice.
 */
const syncBirthdaysToCalendar = async () => {
    const withBirthdays = people.value.filter(p => parseAnnualDate(p.properties?.birthday ?? ''));
    if (withBirthdays.length === 0) return;

    let synced = 0;
    for (const person of withBirthdays) {
        const date = parseAnnualDate(person.properties.birthday)!;
        const month = String(date.month).padStart(2, '0');
        const day = String(date.day).padStart(2, '0');
        // Anchored to a leap year so a 29 February birthday can be written at
        // all; the recurrence engine moves it to the 28th in the years that
        // do not have one.
        const anchor = `2024-${month}-${day}`;

        try {
            await ns.writeNode({
                relPath: `Events/birthday-${slugForPerson(person)}.md`,
                title: `🎂 ${person.title}`,
                nodeType: 'event',
                properties: {
                    is_all_day: true,
                    start_at: anchor,
                    end_at: anchor,
                    recurrence: 'yearly',
                    tags: ['birthday', 'people'],
                    source_person_id: person.id,
                    source_person: person.title,
                },
                content: `Birthday of [${person.title}](synabit://person/${person.id}).`,
                eventType: 'created',
                silent: true,
            });
            synced++;
        } catch (e) {
            logger.error(`Failed to sync birthday for ${person.title}`, e);
        }
    }
    logger.info(`Synced ${synced} birthdays to calendar`);
};

/**
 * What to call a person in a link.
 *
 * Their `node_id`, which follows them when their file moves. Somebody whose
 * file has not been written since identities landed has none yet; their path
 * is used, and the link is rewritten as an identity the next time it is
 * touched.
 */
const identityOf = (person: any): string => person?.properties?.node_id || person?.id;

/**
 * A stable file name for a person's derived entries.
 *
 * Taken from their id, not their name: a name changes, and a path built from
 * one leaves the old file behind as a duplicate the next time it is written.
 */
const slugForPerson = (person: any): string =>
    person.id.replace(/\.md$/, '').replace(/[^a-zA-Z0-9]+/g, '-').toLowerCase();

// --- Person-to-Person Linking ---
const showLinkModal = ref(false);
const editLinkTargetId = ref<string | undefined>(undefined);

const openEditLink = (targetId: string) => {
    editLinkTargetId.value = targetId;
    showLinkModal.value = true;
};

const closeLinkModal = () => {
    showLinkModal.value = false;
    editLinkTargetId.value = undefined;
};

const linkPerson = async (targetPerson: any, relationType: string) => {
    if (!selectedPerson.value) return;
    const src = selectedPerson.value;
    const srcProps = { ...(src.properties || {}) };
    const srcConns: Connection[] = [...(srcProps.connections || [])];
    
    // Update if exists, otherwise push
    const targetNames = namesFor(targetPerson);
    const existingIdx = srcConns.findIndex(c => pointsAt(c, targetNames));
    if (existingIdx >= 0) {
        srcConns[existingIdx].relation_type = relationType;
    } else {
        // The other person's identity, not their path: a path breaks the
        // moment they are moved or renamed. No name is stored either — it is
        // read from their own node when the link is drawn.
        srcConns.push({ person_id: identityOf(targetPerson), relation_type: relationType });
    }
    srcProps.connections = srcConns;

    // `relations` used to hold the same links again as markdown mentions,
    // purely so the edge index would notice them. It reads `connections`
    // directly now, so the duplicate goes — and goes for good, not just for
    // new links.
    srcProps.relations = null;

    try {
        await ns.writeNode({
            relPath: src.id,
            title: src.title,
            nodeType: 'person',
            properties: srcProps,
        });

        // Bidirectional: also add connection on target ONLY if it doesn't exist
        // We do NOT update the target's existing relation, to allow asymmetric labels (e.g. Mother -> Child)
        const tgtProps = { ...(targetPerson.properties || {}) };
        
        const REVERSE_RELATIONS: Record<string, string> = {
            'friend': 'friend',
            'family': 'family',
            'colleague': 'colleague',
            'partner': 'partner',
            'mentor': 'mentee',
            'mentee': 'mentor',
            'neighbor': 'neighbor',
            'client': 'provider',
            'introduced_by': 'introduced'
        };
        const reverseType = REVERSE_RELATIONS[relationType] || 'linked';
        
        const srcNames = namesFor(src);
        const tgtConns: Connection[] = [...(tgtProps.connections || [])];
        if (!tgtConns.some(c => pointsAt(c, srcNames))) {
            tgtConns.push({ person_id: identityOf(src), relation_type: reverseType });
            tgtProps.connections = tgtConns;
            tgtProps.relations = null;
            await ns.writeNode({
                relPath: targetPerson.id,
                title: targetPerson.title,
                nodeType: 'person',
                properties: tgtProps,
            });
        }

        showLinkModal.value = false;
        await fetchPeople();
    } catch (e) {
        logger.error('Failed to link person', e);
    }
};

const unlinkPerson = async (targetPersonId: string) => {
    if (!selectedPerson.value) return;
    const src = selectedPerson.value;
    const targetPerson = people.value.find(p => p.id === targetPersonId);

    try {
        // A link is held on both ends, so unlinking is two writes: take the
        // target out of the source, and the source out of the target.
        const patches = [
            ...linkRemovalPatches([src], targetPerson ?? targetPersonId),
            ...(targetPerson ? linkRemovalPatches([targetPerson], src) : []),
        ];
        for (const patch of patches) {
            await ns.writeNode({
                relPath: patch.id,
                title: patch.title,
                nodeType: 'person',
                properties: patch.properties,
            });
        }

        await fetchPeople();
    } catch (e) {
        logger.error('Failed to unlink person', e);
    }
};

/**
 * Delete a person, and take their links with them.
 *
 * A connection is recorded on both ends. Deleting only the node left the
 * other end pointing at a file that no longer exists — an orphan the graph
 * still drew, using the name it had cached, for somebody who had been
 * deleted. Nothing ever cleared those.
 */
const deletePerson = async (person: any) => {
    if (!person || person.properties?.is_owner) return;

    const yes = await ask(
        `This will permanently delete "${person.title}" and all associated data. This action cannot be undone.`,
        { title: 'Delete contact?', kind: 'warning', okLabel: 'Delete', cancelLabel: 'Cancel' }
    );
    if (!yes) return;

    try {
        // Everyone who names them, before the node goes.
        for (const patch of linkRemovalPatches(people.value, person)) {
            await ns.writeNode({
                relPath: patch.id,
                title: patch.title,
                nodeType: 'person',
                properties: patch.properties,
            });
        }

        // The birthday entry on the calendar is derived from this person and
        // has nobody left to be about. Nothing used to clear it, so deleting
        // somebody left their birthday coming round every year forever.
        try {
            await ns.deleteNode({ relPath: `Events/birthday-${slugForPerson(person)}.md`, silent: true });
        } catch {
            // There may not be one; that is the ordinary case.
        }

        await ns.deleteNode({ relPath: person.id });
        if (selectedPerson.value?.id === person.id) selectedPerson.value = null;
        await fetchPeople();
    } catch (e) {
        logger.error('Failed to delete person', e);
    }
};

defineExpose({ openPersonById });
</script>

<template>
    <div class="h-full flex bg-base dark:bg-base-dark text-text dark:text-text-dark overflow-hidden relative">

        <div v-if="isMobile && isSidebarOpen" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-[48]" @click="isSidebarOpen = false" />

        <!-- LEFT PANEL: People List -->
        <div v-show="!isMobile || isSidebarOpen" class="w-80 flex-shrink-0 border-r border-border dark:border-border-dark flex flex-col bg-surface dark:bg-surface-dark absolute md:relative z-[49] h-full shadow-lg md:shadow-none">
            <!-- Header -->
            <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0" data-tauri-drag-region>
                <div class="flex items-center gap-2 font-semibold">
                    <NavButtons />
                    <Users class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark" />
                    <span>{{ $t('people.people') }}</span>
                </div>
                <div class="flex items-center gap-1">
                    <button @click="openNewModal" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-blue-500" :title="$t('people.add_contact')">
                        <Plus class="w-5 h-5" />
                    </button>
                    <button @click="showImportModal = true" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-gray-500 hover:text-blue-500" :title="$t('people.import_contacts')">
                        <Upload class="w-4 h-4" />
                    </button>
                    <button @click="exportAll" :disabled="exchange.busy.value" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-gray-500 hover:text-blue-500 disabled:opacity-40" :title="$t('people.export_contacts')">
                        <Download class="w-4 h-4" />
                    </button>
                    <button @click="syncBirthdaysToCalendar" class="p-1.5 hover:bg-gray-200 dark:hover:bg-gray-800 rounded-lg transition-colors text-pink-500" :title="$t('people.sync_birthdays')">
                        <CalendarPlus class="w-4 h-4" />
                    </button>
                </div>
            </div>

            <!-- Search + Sort -->
            <div class="p-3 border-b border-border dark:border-border-dark space-y-2">
                <div class="relative">
                    <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                    <input v-model="searchQuery" type="text" :placeholder="$t('people.search_btn')" class="w-full pl-9 pr-3 py-1.5 bg-gray-100 dark:bg-gray-800 border-none rounded-lg text-sm focus:ring-2 focus:ring-blue-500 outline-none transition-all" />
                </div>
                <div class="flex items-center justify-between">
                    <button @click="cycleSortMode" class="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400 hover:text-blue-500 transition-colors px-1.5 py-1 rounded">
                        <ArrowUpDown class="w-3 h-3" /> {{ sortLabel }}
                    </button>
                    <div class="flex items-center gap-2">
                        <div v-if="needsAttentionCount > 0" class="flex items-center gap-1 text-xs">
                            <AlertCircle class="w-3 h-3 text-orange-500" />
                            <span class="text-orange-500 font-medium">{{ needsAttentionCount }}</span>
                        </div>
                        <button @click="selectedPerson = null" class="text-[10px] text-blue-500 hover:text-blue-600 font-medium px-1.5 py-1">{{ $t('people.show_all') }}</button>
                    </div>
                </div>
            </div>

            <!-- Saved segments -->
            <div v-if="segments.length > 0 || people.length > 3" class="px-3 pb-2 flex items-center gap-1.5 flex-wrap">
                <button @click="activeSegmentId = null"
                    :class="['px-2 py-0.5 text-[11px] font-medium rounded-md border transition-colors',
                        activeSegmentId === null ? 'bg-blue-500 text-white border-blue-500'
                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-border dark:border-border-dark hover:border-blue-300']">
                    {{ $t('people.everyone') }}
                </button>
                <button v-for="segment in segments" :key="segment.id"
                    @click="activeSegmentId = segment.id"
                    @dblclick="editingSegment = segment; showSegmentModal = true"
                    :class="['px-2 py-0.5 text-[11px] font-medium rounded-md border transition-colors truncate max-w-[9rem]',
                        activeSegmentId === segment.id ? 'bg-blue-500 text-white border-blue-500'
                        : 'bg-white dark:bg-[#1e1e1e] text-gray-500 border-border dark:border-border-dark hover:border-blue-300']"
                    :title="segment.name">
                    {{ segment.name }}
                </button>
                <button @click="editingSegment = null; showSegmentModal = true"
                    class="px-1.5 py-0.5 text-[11px] rounded-md text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/20 transition-colors"
                    :title="$t('people.new_segment')">
                    <Plus class="w-3 h-3" />
                </button>
            </div>

            <!-- List -->
            <div class="flex-1 overflow-y-auto p-2">
                <!-- Reminders Widget -->
                <RemindersWidget :people="people" @select-person="selectPerson" @updated="fetchPeople" />

                <div v-if="loading" class="flex justify-center p-4">
                    <div class="animate-spin rounded-full h-5 w-5 border-b-2 border-blue-500"></div>
                </div>
                <div v-else-if="people.length === 0" class="text-center px-4 py-8">
                    <Users class="w-8 h-8 mx-auto text-gray-300 dark:text-gray-600" />
                    <p class="mt-3 text-sm font-medium">{{ $t('people.no_people_yet') }}</p>
                    <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">{{ $t('people.no_people_yet_desc') }}</p>
                    <button @click="showImportModal = true" class="mt-4 w-full px-3 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg text-sm font-medium transition-colors">
                        {{ $t('people.import_contacts') }}
                    </button>
                    <button @click="openNewModal" class="mt-2 w-full px-3 py-2 text-sm text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">
                        {{ $t('people.add_one_by_hand') }}
                    </button>
                </div>
                <div v-else-if="sidebarPeople.length === 0" class="text-center p-4 text-sm text-gray-500">{{ $t('people.no_contacts') }}</div>
                <!--
                    One tab stop, and the arrows move within it. With two
                    thousand contacts, Tab through every row is not a way
                    through a list.
                -->
                <div v-else class="space-y-1" role="listbox" :aria-label="$t('people.people')"
                    @keydown="listKeys.onKeydown">
                    <button
                        v-for="(person, index) in sidebarPeople" :key="person.id"
                        :ref="(el: any) => { if (index === listKeys.activeIndex.value) activeRow = el; }"
                        role="option"
                        :aria-selected="selectedPerson?.id === person.id"
                        :tabindex="listKeys.tabIndexFor(index)"
                        @focus="listKeys.onRowFocus(index)"
                        @click="selectPerson(person)"
                        :class="['w-full text-left px-3 py-2 rounded-lg flex items-center gap-3 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-blue-500',
                            selectedPerson?.id === person.id
                                ? 'bg-blue-50 dark:bg-blue-900/30 ring-1 ring-blue-500/50'
                                : 'hover:bg-gray-100 dark:hover:bg-gray-800/50'
                        ]"
                    >
                        <!-- Avatar or Initials -->
                        <div class="w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold flex-shrink-0 overflow-hidden relative"
                             :class="person.properties?.avatar ? '' : 'bg-gradient-to-br from-gray-200 to-gray-300 dark:from-gray-700 dark:to-gray-800 text-gray-700 dark:text-gray-300'">
                            <img v-if="getAvatarSrc(person)" :src="getAvatarSrc(person)" class="w-full h-full object-cover" />
                            <span v-else>{{ getInitials(getDisplayName(person)) }}</span>
                            <!-- Health dot -->
                            <div v-if="getContactHealthDot(person)" :class="['absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-white dark:border-gray-900', getContactHealthDot(person)]"></div>
                        </div>
                        <div class="flex-1 min-w-0">
                            <h4 class="font-medium text-sm truncate">{{ getDisplayName(person) }}</h4>
                            <p v-if="getPersonDetail(person, 'company')" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Building class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ getPersonDetail(person, 'company') }}</span>
                            </p>
                            <p v-else-if="relationshipLabel(person)" class="text-xs text-gray-400 truncate mt-0.5 capitalize">{{ relationshipLabel(person) }}</p>
                            <p v-else-if="person.properties?.tags?.length" class="text-xs text-gray-500 dark:text-gray-400 truncate flex items-center gap-1 mt-0.5">
                                <Hash class="w-3 h-3 flex-shrink-0" />
                                <span class="truncate">{{ person.properties.tags.join(', ') }}</span>
                            </p>
                        </div>
                    </button>
                    
                    <button v-if="filteredPeople.length > 20" @click="selectedPerson = null" class="w-full text-center py-2.5 mt-2 text-xs font-medium text-blue-500 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors">
                        {{ $t('people.show_more', { count: filteredPeople.length - 20 }) }}
                    </button>
                </div>
            </div>
        </div>

        <!-- RIGHT PANEL -->
        <div class="flex-1 flex flex-col bg-base dark:bg-base-dark overflow-hidden relative">
            <!-- All People (when no person selected) -->
            <PeopleManager v-if="!selectedPerson"
                :people="filteredPeople"
                :vault-path="vaultPath"
                @select="selectPerson"
                @edit="(p: any) => editPerson(p)"
                @delete="deletePerson"
            />

            <div v-if="selectedPerson" class="flex-1 flex flex-col overflow-hidden">
                <!-- Profile Header -->
                <div class="flex-shrink-0 px-4 md:px-8 pt-4 md:pt-8 pb-4">
                    <div class="md:hidden mb-4">
                        <button @click="isSidebarOpen = true" class="flex items-center gap-1.5 text-blue-500 hover:text-blue-600 font-medium">
                            <PanelLeft class="w-5 h-5" /> {{ $t('people.all_people') || 'All People' }}
                        </button>
                    </div>
                    <div class="flex items-start gap-3 md:gap-5 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 md:p-5 shadow-sm relative group">
                        <button @click="editPerson(selectedPerson)" class="absolute top-4 right-4 p-2 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30 rounded-lg md:opacity-0 opacity-100 group-hover:opacity-100 transition-all" :aria-label="$t('people.edit')">
                            <Edit2 class="w-4 h-4" />
                        </button>

                        <!-- Avatar -->
                        <div class="w-16 h-16 md:w-20 md:h-20 rounded-2xl flex items-center justify-center text-xl md:text-2xl font-bold flex-shrink-0 overflow-hidden shadow-md"
                             :class="getAvatarSrc(selectedPerson) ? '' : 'bg-gradient-to-br from-blue-500 to-purple-600 text-white'">
                            <img v-if="getAvatarSrc(selectedPerson)" :src="getAvatarSrc(selectedPerson)" class="w-full h-full object-cover" />
                            <span v-else>{{ getInitials(getDisplayName(selectedPerson)) }}</span>
                        </div>

                        <div class="flex-1 min-w-0">
                            <div class="flex items-center gap-3 mb-1">
                                <h1 class="text-xl font-bold text-gray-900 dark:text-white truncate">{{ getDisplayName(selectedPerson) }}</h1>
                                <span v-if="relationshipLabel(selectedPerson)" class="px-2 py-0.5 text-xs font-medium rounded-full bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300 capitalize flex-shrink-0">
                                    {{ relationshipLabel(selectedPerson) }}
                                </span>
                            </div>

                            <!-- Subtitle from details -->
                            <p v-if="getPersonDetail(selectedPerson, 'company') || getPersonDetail(selectedPerson, 'role')" class="text-sm text-gray-600 dark:text-gray-300 flex items-center gap-1.5 mb-3">
                                <Briefcase v-if="getPersonDetail(selectedPerson, 'role')" class="w-3.5 h-3.5 opacity-60" />
                                <span v-if="getPersonDetail(selectedPerson, 'role')">{{ getPersonDetail(selectedPerson, 'role') }}</span>
                                <span v-if="getPersonDetail(selectedPerson, 'role') && getPersonDetail(selectedPerson, 'company')" class="text-gray-400">@</span>
                                <span v-if="getPersonDetail(selectedPerson, 'company')" class="text-blue-600 dark:text-blue-400 font-medium">{{ getPersonDetail(selectedPerson, 'company') }}</span>
                            </p>

                            <!-- Details info row -->
                            <div class="flex flex-wrap gap-x-5 gap-y-1.5 text-xs text-gray-500 dark:text-gray-400">
                                <template v-for="d in (selectedPerson.properties?.details || [])" :key="d.label + d.value">
                                    <a v-if="d.type === 'email'" :href="'mailto:' + d.value" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                        <Mail class="w-3.5 h-3.5" /> <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </a>
                                    <a v-else-if="d.type === 'phone'" :href="'tel:' + d.value" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                        <Phone class="w-3.5 h-3.5" /> <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </a>
                                    <span v-else-if="d.type === 'url'" class="flex items-center gap-1.5">
                                        <span class="opacity-50">{{ d.label }}:</span>
                                        <a :href="d.value" target="_blank" class="hover:text-blue-500 transition-colors truncate max-w-[180px]">{{ d.value.replace(/^https?:\/\//, '') }}</a>
                                    </span>
                                    <span v-else class="flex items-center gap-1.5">
                                        <span class="opacity-50">{{ d.label }}:</span> {{ d.value }}
                                    </span>
                                </template>
                                <!-- Legacy fallbacks -->
                                <a v-if="!selectedPerson.properties?.details?.length && selectedPerson.properties?.email" :href="'mailto:' + selectedPerson.properties.email" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                    <Mail class="w-3.5 h-3.5" /> {{ selectedPerson.properties.email }}
                                </a>
                                <a v-if="!selectedPerson.properties?.details?.length && selectedPerson.properties?.phone" :href="'tel:' + selectedPerson.properties.phone" class="flex items-center gap-1.5 hover:text-blue-500 transition-colors">
                                    <Phone class="w-3.5 h-3.5" /> {{ selectedPerson.properties.phone }}
                                </a>
                                <span v-if="selectedPerson.properties?.birthday" class="flex items-center gap-1.5">
                                    <Gift class="w-3.5 h-3.5 text-pink-500" /> {{ selectedPerson.properties.birthday }}
                                </span>
                            </div>

                            <!-- Tags -->
                            <div v-if="selectedPerson.properties?.tags?.length > 0" class="flex flex-wrap gap-1.5 mt-3">
                                <span v-for="tag in selectedPerson.properties.tags" :key="tag"
                                    :class="['px-2 py-0.5 text-xs font-medium rounded-md flex items-center gap-1', getTagColor(tag)]">
                                    <Hash class="w-2.5 h-2.5 opacity-50" /> {{ tag }}
                                </span>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- Tab Bar -->
                <div class="flex-shrink-0 px-4 md:px-8">
                    <div class="flex flex-wrap items-center gap-1 border-b border-border dark:border-border-dark pb-1 md:pb-0">
                        <button
                            v-for="tab in tabs" :key="tab.id"
                            @click="activeTab = tab.id as any"
                            :class="[
                                'flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 transition-all -mb-px',
                                activeTab === tab.id
                                    ? 'border-blue-500 text-blue-600 dark:text-blue-400'
                                    : 'border-transparent text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 hover:border-gray-300 dark:hover:border-gray-600'
                            ]"
                        >
                            <component :is="tab.icon" class="w-4 h-4" />
                            {{ tab.label }}
                        </button>
                        <div class="w-full md:w-auto md:ml-auto md:-mb-px flex items-center gap-1 mt-1 md:mt-0 justify-end">
                            <button @click="showLinkModal = true" class="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-purple-500 hover:bg-purple-50 dark:hover:bg-purple-900/20 rounded-lg transition-colors">
                                <UserPlus class="w-3.5 h-3.5" /> {{ $t('people.link_person') }}
                            </button>
                            <button @click="showGiftModal = true" class="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-pink-500 hover:bg-pink-50 dark:hover:bg-pink-900/20 rounded-lg transition-colors">
                                <Gift class="w-3.5 h-3.5" /> {{ $t('people.log_gift') }}
                            </button>
                        </div>
                    </div>
                </div>

                <!-- Tab Content -->
                <div class="flex-1 overflow-y-auto hidden-scrollbar relative bg-surface dark:bg-surface-dark p-4 md:p-8">
                    <div class="max-w-3xl mx-auto">
                        <OverviewTab v-if="activeTab === 'overview'" :person="selectedPerson" @open-linked-node="openLinkedNode" @open-node="(id: string, type: string) => emit('open-node', id, type)" />
                        <TimelineTab v-else-if="activeTab === 'timeline'" :person="selectedPerson" :vault-path="vaultPath" :linked-nodes="linkedNodes" :all-debts="allDebts" :all-transactions="allTransactions" @updated="handleTimelineUpdated" @open-linked-node="openLinkedNode" />
                        <NotesTab v-else-if="activeTab === 'notes'" :person="selectedPerson" :linked-nodes="linkedNodes" :loading-links="loadingLinks" @open-linked-node="openLinkedNode" />
                        <GraphTab v-else-if="activeTab === 'graph'" :person="selectedPerson" :all-people="people" :vault-path="vaultPath" @select-person="selectPerson" @unlink="unlinkPerson" @edit-link="openEditLink" />
                    </div>
                </div>
            </div>
        </div>

        <SegmentModal
            v-if="showSegmentModal"
            :segment="editingSegment"
            :all-relationships="allRelationships"
            :all-tags="allTags"
            @close="showSegmentModal = false"
            @save="saveSegment"
            @delete="deleteSegment"
        />

        <ImportContactsModal
            v-if="showImportModal"
            :vault-path="vaultPath"
            @close="showImportModal = false"
            @imported="fetchPeople"
        />

        <PersonModal
            v-if="showModal"
            :person="selectedPerson"
            :vault-path="vaultPath"
            :top-relationships="topRelationships"
            :all-relationships="allRelationships"
            @close="showModal = false"
            @saved="fetchPeople"
            @delete="deletePerson"
        />

        <GiftModal
            v-if="showGiftModal && selectedPerson"
            :person="selectedPerson"
            @close="showGiftModal = false"
            @save="handleGiftSaved"
        />

        <LinkPersonModal
            v-if="showLinkModal && selectedPerson"
            :vault-path="vaultPath"
            :person="selectedPerson"
            :all-people="people"
            :preselected-person-id="editLinkTargetId"
            @close="closeLinkModal"
            @link="linkPerson"
        />
    </div>
</template>
