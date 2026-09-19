import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import LensBar from '../LensBar.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../../utils/logger', () => ({
    logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));
vi.mock('../../../../composables/useNodeService', () => ({
    useNodeService: () => ({ writeNode: vi.fn(), trashNode: vi.fn() }),
}));

/** What a timeline question answers with: an event id, and a note to open. */
const EVENTS = {
    columns: ['when', 'title', 'who'],
    rows: [
        {
            id: 'Notes/2019-11-05.md#moment#0',
            node_type: 'note',
            title: 'Gặp Khánh ở quán quen',
            cells: ['2019-11-05', 'Gặp Khánh ở quán quen', 'uuid-khanh'],
            open: 'Notes/2019-11-05.md',
        },
    ],
    total: 1,
    query_time_ms: 2,
};

const EMPTY_SHELF = { columns: [], rows: [], total: 0, query_time_ms: 0 };

const mountBar = () => {
    vi.mocked(invoke).mockImplementation(async (_command: string, args?: unknown) => {
        const q = (args as { query?: string })?.query ?? '';
        return q.includes('is:lens') ? EMPTY_SHELF : EVENTS;
    });
    return mount(LensBar, {
        props: { vaultPath: '/vault' },
        global: { plugins: [i18n] },
    });
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('Asking Nexus a question', () => {
    it('sends the words through the one query command', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh when:2019');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('run_node_query', {
            vaultPath: '/vault',
            query: 'with:khánh when:2019',
            offset: 0,
        });
    });

    /// The claim the whole design rests on: `TableView` was written for notes,
    /// long before events had a query language, and it draws this without
    /// being taught anything.
    it('draws a timeline answer with the view that was written for notes', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        const shown = wrapper.find('[data-answer]').text();
        expect(shown).toContain('Gặp Khánh ở quán quen');
        expect(wrapper.find('[data-found]').text()).toContain('1');
    });

    it('opens the note a row came from, not the event id', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        const table = wrapper.findComponent({ name: 'TableView' });
        table.vm.$emit('open', EVENTS.rows[0]);
        expect(wrapper.emitted('open')?.[0]).toEqual(['Notes/2019-11-05.md', 'note']);
    });

    /// Refusing and saying why is the point of refusing at all.
    it('says why when the engine will not answer', async () => {
        // Only the question under test refuses. A mock that threw for
        // anything it did not recognise would fail on a call this test never
        // meant to be about, and report it as this test's failure.
        vi.mocked(invoke).mockImplementation(async (_c: string, args?: unknown) => {
            const q = (args as { query?: string })?.query ?? '';
            if (q.startsWith('when:hôm')) throw new Error("'hôm-nào-đó' is not a time.");
            return EMPTY_SHELF;
        });
        const wrapper = mount(LensBar, {
            props: { vaultPath: '/vault' },
            global: { plugins: [i18n] },
        });
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('when:hôm-nào-đó');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        expect(wrapper.find('[data-refused]').text()).toContain('is not a time');
        expect(wrapper.find('[data-found]').exists()).toBe(false);
    });

    it('will not ask nothing', async () => {
        const wrapper = mountBar();
        await flushPromises();
        expect(wrapper.find('[data-run]').attributes('disabled')).toBeDefined();
        await wrapper.find('[data-ask]').setValue('  ');
        expect(wrapper.find('[data-run]').attributes('disabled')).toBeDefined();
    });
});
