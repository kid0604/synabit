import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import NexusApp from '../NexusApp.vue';
import { i18n } from '../../../i18n';

/**
 * One box, two tables.
 *
 * What was typed is asked of `nodes` and of `events`, and the two answers sit
 * in two tabs. These lock the three things that decide whether that is worth
 * having: the timeline really is asked, each half's refusal reaches its own
 * tab, and a refusal is never drawn as a zero.
 */

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), convertFileSrc: (p: string) => p }));
vi.mock('../../../utils/logger', () => ({
    logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

const NODE_HITS = {
    results: [
        {
            id: 'Notes/an.md',
            item_type: 'note',
            title: 'Ăn tối với Minh',
            snippet: 'chỗ quen',
            tags: [],
            date: '2026-05-29',
            path: 'Notes/an.md',
            score: 1,
        },
    ],
    total_count: 1,
    query_time_ms: 3,
};

const EVENT_ROWS = {
    columns: ['when', 'title', 'who'],
    rows: [
        {
            id: 'Notes/an.md#moment#0',
            node_type: 'note',
            title: 'Ăn tối với Minh',
            cells: ['2026-05-29', 'Ăn tối với Minh', ''],
            open: 'Notes/an.md',
        },
        {
            id: 'Notes/an.md#moment#1',
            node_type: 'note',
            title: 'Ăn trưa với Khánh',
            cells: ['2026-04-02', 'Ăn trưa với Khánh', ''],
            open: 'Notes/an.md',
        },
    ],
    total: 2,
    query_time_ms: 5,
};

/**
 * What `AppError::Refused` serialises to — `REFUSED:` and all.
 *
 * Written out rather than faked loosely, because the prefix is the whole of
 * how `refusalText` tells a refusal from a crash: without it the screen falls
 * back to `said()` and shows the raw shape, which is the bug §27 of
 * `docs/query-grammar-2026-09-20.md` was written to close.
 */
const REFUSED = (name: string, ...args: string[]) => ({
    code: `REFUSED:${name}`,
    message: `the engine's English for ${name}`,
    args,
});

/** Every query `run_node_query` was handed, in order. */
let asked: string[] = [];

interface Answers {
    events?: () => unknown;
    nodes?: () => unknown;
    /** Overrides the whole extraction status, for an empty queue. */
    status?: () => unknown;
}

const mountNexus = (answers: Answers = {}) => {
    asked = [];
    vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
        const query = (args as { query?: string })?.query ?? '';
        if (command === 'run_node_query') {
            asked.push(query);
            return (answers.events ?? (() => EVENT_ROWS))();
        }
        if (command === 'search_nexus') return (answers.nodes ?? (() => NODE_HITS))();
        if (command === 'search_nexus_ids') return ['Notes/an.md'];
        if (command === 'get_nexus_items') return [];
        if (command === 'get_nexus_graph_data') return { nodes: [], links: [] };
        if (command === 'timeline_extract_status') return answers.status ? answers.status() : {
            config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false },
            syn_enabled: true, provider: 'ollama', local: true, model: 'q', desktop: true,
            running: false, pending: 4, stale: 0, old_version: 0, done: 0,
            estimate_ms: 1000, estimate_all_ms: 1000, estimate_measured: true,
            unreadable: [], proposals: [
                { id: 'p1', node_id: 'Notes/a.md', node_title: 'a', node_type: 'note', recorded: '2026-04-30',
                  happened_from: '2026-04-30', happened_to: '2026-04-30', precision: 'day',
                  title: 'Một chuyện', people: [], names: [], quote: 'một câu', confidence: 0.9, category: null, amount: null, about: [], time: null, place: null, date_basis: null,
                  model: 'q', stale: false },
                { id: 'p2', node_id: 'Notes/b.md', node_title: 'b', node_type: 'note', recorded: '2026-04-02',
                  happened_from: '2026-04-02', happened_to: '2026-04-02', precision: 'day',
                  title: 'Chuyện khác', people: [], names: [], quote: 'câu khác', confidence: 0.9, category: null, amount: null, about: [], time: null, place: null, date_basis: null,
                  model: 'q', stale: false },
            ],
        };
        if (command === 'syn_check_status') return { connected: false };
        if (command === 'syn_get_settings') return { default_model: null };
        return null;
    });

    return mount(NexusApp, {
        props: { vaultPath: '/vault' },
        global: {
            plugins: [i18n],
            stubs: {
                // Only the canvas and the app-wide chrome. The rest is what
                // is being tested — and the panels that used to be listed
                // here no longer exist.
                GraphView: { name: 'GraphView', template: '<div/>' },
                NexusTagManager: true,
                NavButtons: true,
            },
        },
    });
};

/** Type into the box and let both halves answer. */
const search = async (wrapper: ReturnType<typeof mountNexus>, text: string) => {
    await wrapper.find('input[type="text"]').setValue(text);
    // The box is debounced by 250ms before either half is asked.
    vi.advanceTimersByTime(300);
    await flushPromises();
};

beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
    vi.useFakeTimers();
});

describe('Searching answers as both tables', () => {
    it('asks the timeline the same words, named as moments', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        expect(asked).toContain('moments ăn limit:1000');
    });

    /// The source is the first word or it is not the source, so a question
    /// that already named one must have it replaced, not stacked on.
    it('replaces a source the person wrote rather than stacking one on it', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'nodes ăn');
        expect(asked).toContain('moments ăn limit:1000');
        expect(asked.some(q => q.startsWith('moments nodes'))).toBe(false);
    });

    it('shows both counts before either tab is pressed', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        const counts = wrapper.findAll('[data-tab-count]').map(c => c.text());
        expect(counts).toEqual(['1', '2']);
    });

    /// Nodes is what the box has always answered, so it stays the one in front.
    it('opens on the node side', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        expect(wrapper.find('[data-search-tab][data-tab="nodes"]').attributes('aria-pressed')).toBe('true');
        expect(wrapper.find('[data-events-results]').exists()).toBe(false);
    });

    it('draws the events down the days once that tab is pressed', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        const drawn = wrapper.find('[data-events-results]');
        expect(drawn.exists()).toBe(true);
        expect(drawn.text()).toContain('Ăn trưa với Khánh');
    });
});

describe('Each half refuses on its own', () => {
    /// `#tag` is a field of a node, so the timeline says so — and it must say
    /// so on its own tab without taking the node answer down with it.
    it('keeps a timeline refusal out of the node tab', async () => {
        const wrapper = mountNexus({
            events: () => {
                throw REFUSED('node_field_on_events', '#');
            },
        });
        await search(wrapper, '#gia-đình');

        expect(wrapper.find('[data-search-refused]').exists()).toBe(false);
        expect(wrapper.findAll('[data-tab-count]')).toHaveLength(1);

        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        // The locale sentence with the field filled in — not the raw
        // shape. The `REFUSED:` prefix is what picks one over the other, and
        // without it the screen shows JSON, which is the bug §27 of
        // `docs/query-grammar-2026-09-20.md` closed.
        const why = wrapper.find('[data-events-refused]').text();
        expect(why).toContain('about the timeline');
        expect(why).toContain('#');
        expect(why).not.toContain('{0}');
        expect(why).not.toContain('REFUSED');
    });

    /// And the other way: `when:` is beyond the FTS box but is exactly what
    /// the timeline is for, so the node tab refuses and the event tab answers.
    it('keeps a node refusal out of the timeline tab', async () => {
        const wrapper = mountNexus({
            nodes: () => {
                throw REFUSED('on_the_timeline_tab', 'when:');
            },
        });
        await search(wrapper, 'when:2019');

        const why = wrapper.find('[data-search-refused]').text();
        // It points at the tab beside it, which is where that word really is
        // answered. It used to point at a query bar — and that bar is gone.
        expect(why).toContain('Timeline tab');
        expect(why).toContain('when:');
        expect(why).not.toContain('{0}');
        expect(why).not.toContain('REFUSED');

        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        expect(wrapper.find('[data-events-refused]').exists()).toBe(false);
        expect(wrapper.find('[data-events-results]').text()).toContain('Ăn trưa với Khánh');
    });

    /// A refusal is not a zero. Drawing one as `0` would say the timeline
    /// holds nothing about this, when it was never asked.
    it('draws no count on a tab that refused', async () => {
        const wrapper = mountNexus({
            events: () => {
                throw REFUSED('node_field_on_events', '#');
            },
        });
        await search(wrapper, '#gia-đình');
        const tabs = wrapper.findAll('[data-search-tab]');
        expect(tabs[0].find('[data-tab-count]').exists()).toBe(true);
        expect(tabs[1].find('[data-tab-count]').exists()).toBe(false);
    });

    /// A refused question leaves no results, and the empty state used to win
    /// the race — so «I cannot ask that» read as «there is nothing there».
    it('says it cannot ask, rather than that nothing matched', async () => {
        const wrapper = mountNexus({
            nodes: () => {
                throw REFUSED('on_the_timeline_tab', 'when:');
            },
        });
        await search(wrapper, 'when:2019');
        expect(wrapper.find('[data-search-refused]').exists()).toBe(true);
        expect(wrapper.text()).not.toContain('No results found');
    });
});

describe('The pane behind the tabs', () => {
    /// The word "Timeline" used to name two controls on one screen — a tab
    /// and the pane's own toggle — drawing the same rows twice.
    it('takes the pane back to the graph while a question is open', async () => {
        const wrapper = mountNexus();
        expect(wrapper.find('[data-shown-as]').exists()).toBe(true);

        await search(wrapper, 'ăn');
        expect(wrapper.find('[data-shown-as]').exists()).toBe(false);
        expect(wrapper.find('[data-timeline-pane]').exists()).toBe(false);
    });

    /// And browsing keeps both shapes, because with nothing asked there are
    /// no tabs to read the answer in.
    it('keeps both shapes for somebody browsing', async () => {
        const wrapper = mountNexus();
        await wrapper.find('[data-shape="timeline"]').trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-timeline-pane]').exists()).toBe(true);
    });
});

describe('A timeline count is a floor, not a total', () => {
    /// The vault held a note the extractor had not read, so the tab said 1
    /// where the truth was more — and nothing said so.
    it('owns up to what has not been read into events', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-events-backlog]').text()).toContain('2');
    });

    /// The count is on a button that is always on screen, so it is asked for
    /// once when Nexus opens rather than when a tab is pressed. A queue that
    /// fills itself in the background is no use if you have to go looking.
    it('counts what is waiting as soon as the screen opens', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        expect(vi.mocked(invoke).mock.calls.some(c => c[0] === 'timeline_extract_status')).toBe(true);
        expect(wrapper.find('[data-review-proposals]').text()).toContain('2');
    });

    /// Behind that door are the reading settings and the way to clear the
    /// timeline and start again. It used to appear only when something was
    /// waiting — so the moment the queue emptied, which is exactly when
    /// somebody wants to change how reading works or start over, the door
    /// was gone.
    it('is there with an empty queue too, without a count', async () => {
        const wrapper = mountNexus({
            status: () => ({
                config: { enabled: true, allow_cloud: false, folders: [], tags: [], conversations: false, categories: [] },
                syn_enabled: true, provider: 'ollama', local: true, model: 'q', desktop: true,
                running: false, pending: 0, stale: 0, old_version: 0, done: 12,
                estimate_ms: 0, estimate_all_ms: 0, estimate_measured: true,
                unreadable: [], proposals: [], changes: 0, people: [], moments: {}, categories: [],
            }),
        });
        await flushPromises();
        const door = wrapper.find('[data-review-proposals]');
        expect(door.exists()).toBe(true);
        expect(door.text()).not.toMatch(/\d/);

        await door.trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-review-screen]').exists()).toBe(true);
        // And what is behind it: the settings, and starting again.
        expect(wrapper.find('[data-extract-settings]').exists()).toBe(true);
        expect(wrapper.find('[data-reset-ask]').exists()).toBe(true);
    });

    /// And pressing it opens the review, which is a screen and not a popover.
    it('opens the review over the whole window', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        expect(wrapper.find('[data-review-screen]').exists()).toBe(false);
        await wrapper.find('[data-review-proposals]').trigger('click');
        expect(wrapper.find('[data-review-screen]').exists()).toBe(true);
    });
});

describe('Pressing the graph', () => {
    const personNode = { id: 'People/khanh.md', item_type: 'person', title: 'Khánh', tags: [] };
    const tagNode = { id: 'tag:gia-đình', item_type: 'tag', title: '#gia-đình', tags: [] };
    const noteNode = { id: 'Notes/a.md', item_type: 'note', title: 'A note', tags: [] };

    /// It used to press a bar that only existed inside "Look back", so
    /// anywhere else the optional call did nothing *and* the `return`
    /// swallowed the ordinary open. Two of the most-pressed node types did
    /// nothing at all.
    it('asks about a person by name rather than doing nothing', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        wrapper.findComponent({ name: 'GraphView' }).vm.$emit('node-click', personNode);
        await flushPromises();
        expect((wrapper.find('input[type="text"]').element as HTMLInputElement).value).toBe('Khánh');
    });

    it('asks about a tag as a tag', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        wrapper.findComponent({ name: 'GraphView' }).vm.$emit('node-click', tagNode);
        await flushPromises();
        expect((wrapper.find('input[type="text"]').element as HTMLInputElement).value).toBe('#gia-đình');
    });

    /// Anything else opens the way it always did.
    it('opens anything else', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        wrapper.findComponent({ name: 'GraphView' }).vm.$emit('node-click', noteNode);
        expect(wrapper.emitted('edit-item')?.[0]).toEqual(['Notes/a.md', 'note']);
    });
});

describe('The two doors into the timeline', () => {
    /// They sat at opposite corners — compose bottom-left, proposals up by
    /// the search box — so nothing said they were two halves of one thing.
    it('stand in one frame', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        const doors = wrapper.find('[data-timeline-doors]');
        expect(doors.exists()).toBe(true);
        expect(doors.find('[data-compose-open]').exists()).toBe(true);
        expect(doors.find('[data-review-proposals]').exists()).toBe(true);
    });

    /// The complaint was that every word said "write" and "note", which is
    /// what these are not. Both labels name the thing they make — a moment,
    /// and not an *event*, which is what the calendar calls an appointment.
    it('are both labelled as being about moments, not notes or events', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        const doors = wrapper.find('[data-timeline-doors]').text().toLowerCase();
        expect(doors).toContain('moment');
        expect(doors).not.toContain('note');
        expect(doors).not.toContain('event');
    });

    /// And each surface says so once more where the work happens.
    it('say what they make, where the work happens', async () => {
        const wrapper = mountNexus();
        await flushPromises();
        await wrapper.find('[data-compose-open]').trigger('click');
        expect(wrapper.find('[data-is-an-event]').text().toLowerCase()).toContain('timeline');

        await wrapper.find('[data-review-proposals]').trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-review-screen]').find('[data-is-an-event]').text().toLowerCase())
            .toContain('timeline');
    });
});

describe('The pane follows the tab', () => {
    /// The graph is built from the *node* matches, so on the events tab the
    /// two halves of the screen used to answer two different questions.
    it('draws the events across time when the timeline tab is open', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        expect(wrapper.find('[data-events-pane]').exists()).toBe(false);

        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        expect(wrapper.find('[data-events-pane]').find('[data-over-time]').exists()).toBe(true);

        await wrapper.find('[data-search-tab][data-tab="nodes"]').trigger('click');
        expect(wrapper.find('[data-events-pane]').exists()).toBe(false);
    });

    /// A chart of the first page, sorted newest first, says every busy month
    /// was a recent one.
    it('asks for the whole answer, not the first page', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        expect(asked).toContain('moments ăn limit:1000');
    });

    /// A range picked for one question must not quietly hide rows of the next.
    it('forgets the picked stretch when the question changes', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        wrapper.findComponent({ name: 'EventsOverTime' }).vm.$emit('update:modelValue', { from: '2026-05-01', to: '2026-05-31' });
        await flushPromises();
        expect(wrapper.find('[data-events-narrowed]').exists()).toBe(true);
        expect(wrapper.find('[data-events-results]').text()).not.toContain('Ăn trưa với Khánh');

        await search(wrapper, 'trưa');
        expect(wrapper.find('[data-events-narrowed]').exists()).toBe(false);
    });
});

describe('A search starts where the person was', () => {
    /// Asking from the timeline used to land on the node tab with the graph
    /// in the pane — throwing away which half they were reading.
    it('opens on the events tab when asked from the timeline', async () => {
        const wrapper = mountNexus();
        await wrapper.find('[data-shape="timeline"]').trigger('click');
        await flushPromises();
        await search(wrapper, 'ăn');
        expect(wrapper.find('[data-search-tab][data-tab="events"]').attributes('aria-pressed')).toBe('true');
        expect(wrapper.find('[data-events-pane]').exists()).toBe(true);
    });

    it('opens on the node tab when asked from the graph', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        expect(wrapper.find('[data-search-tab][data-tab="nodes"]').attributes('aria-pressed')).toBe('true');
    });

    /// And clearing the box goes back to the shape of the tab it ended on.
    it('returns to the shape of the tab it finished on', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        await search(wrapper, '');
        await flushPromises();
        expect(wrapper.find('[data-timeline-pane]').exists()).toBe(true);
    });
});

describe('Looking at a long timeline', () => {
    /// Sixty things from the last two months and two from 1991: the axis
    /// used to run 1991 → today, and this year was one column at its edge.
    const RECENT_AND_ANCIENT = (() => {
        const pad = (n: number) => String(n).padStart(2, '0');
        const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
        const days = Array.from({ length: 60 }, (_, i) => iso(new Date(Date.now() - i * 86_400_000)));
        days.push('1991-05-01', '1992-01-01');
        return {
            columns: ['when', 'title'],
            rows: days.map((day, i) => ({ id: `e${i}`, node_type: 'note', title: `Chuyện ${i}`, cells: [day, `Chuyện ${i}`] })),
            total: days.length,
            query_time_ms: 1,
        };
    })();

    it('looks at the last year by default when there is a lot, and says what it left out', async () => {
        const wrapper = mountNexus({ events: () => RECENT_AND_ANCIENT });
        await search(wrapper, 'chuyện');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');

        expect(wrapper.find('[data-preset="1y"]').attributes('aria-pressed')).toBe('true');
        expect(wrapper.find('[data-events-windowed]').text()).toContain('2');
        expect(wrapper.find('[data-events-results]').text()).not.toContain('Chuyện 61');
    });

    /// One press, and the chart and the list both show everything.
    it('shows all of it when asked', async () => {
        const wrapper = mountNexus({ events: () => RECENT_AND_ANCIENT });
        await search(wrapper, 'chuyện');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        await wrapper.find('[data-events-windowed] button').trigger('click');

        expect(wrapper.find('[data-preset="all"]').attributes('aria-pressed')).toBe('true');
        expect(wrapper.find('[data-events-windowed]').exists()).toBe(false);
        expect(wrapper.find('[data-events-results]').text()).toContain('Chuyện 61');
    });

    /// Little to show: all of it, with nothing said about leaving any out.
    it('shows everything when there is little', async () => {
        const wrapper = mountNexus();
        await search(wrapper, 'ăn');
        await wrapper.find('[data-search-tab][data-tab="events"]').trigger('click');
        expect(wrapper.find('[data-preset="all"]').attributes('aria-pressed')).toBe('true');
        expect(wrapper.find('[data-events-windowed]').exists()).toBe(false);
    });
});
