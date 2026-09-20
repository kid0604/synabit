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

        // Pressing the row as a person would, through whichever view the
        // answer chose — rather than poking the component that used to draw it.
        await wrapper.find('[data-dated-row]').trigger('click');
        expect(wrapper.emitted('open')?.[0]).toEqual(['Notes/2019-11-05.md', 'note']);
    });

    /// Refusing and saying why is the point of refusing at all.
    it('says why when the engine will not answer', async () => {
        // Only the question under test refuses. A mock that threw for
        // anything it did not recognise would fail on a call this test never
        // meant to be about, and report it as this test's failure.
        vi.mocked(invoke).mockImplementation(async (_c: string, args?: unknown) => {
            const q = (args as { query?: string })?.query ?? '';
            // The shape `AppError` actually serialises to — `{code, message}`,
            // not an `Error`. Mocking an `Error` here is what let the bug
            // below live: `String({…})` is "[object Object]".
            if (q.startsWith('when:hôm')) {
                // eslint-disable-next-line no-throw-literal
                throw { code: 'GENERAL_ERROR', message: "'hôm-nào-đó' is not a time." };
            }
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

    // ─── The bar as a receipt ───────────────────────────────────

    it('shows what was asked as chips, and lets the words be edited', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh when:2019');
        await flushPromises();

        // Still typing, so still words.
        expect(wrapper.findAll('[data-chip]')).toHaveLength(0);

        // Pressing something elsewhere is what fills the bar.
        await (wrapper.vm as unknown as { press: (k: string, v: string) => Promise<void> })
            .press('with', 'Minh');
        await flushPromises();
        expect(wrapper.findAll('[data-chip]').map(c => c.text())).toEqual([
            expect.stringContaining('khánh'),
            expect.stringContaining('2019'),
            expect.stringContaining('Minh'),
        ]);
    });

    /// §6.1: pressing a person is asking about them, and the gesture *was*
    /// the question — so it runs, rather than waiting for a second press.
    it('runs straight away when something is pressed', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await (wrapper.vm as unknown as { press: (k: string, v: string) => Promise<void> })
            .press('with', 'Khánh');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('run_node_query', {
            vaultPath: '/vault',
            query: 'with:Khánh',
            offset: 0,
        });
        expect(wrapper.find('[data-answer]').exists()).toBe(true);
    });

    it('takes a chip off and asks again without it', async () => {
        const wrapper = mountBar();
        await flushPromises();
        const vm = wrapper.vm as unknown as { press: (k: string, v: string) => Promise<void> };
        await vm.press('with', 'Khánh');
        await vm.press('when', '2019');
        await flushPromises();

        await wrapper.findAll('[data-chip-drop]')[0].trigger('click');
        await flushPromises();
        const calls = vi.mocked(invoke).mock.calls;
        expect(calls[calls.length - 1][1]).toMatchObject({ query: 'when:2019' });
    });

    /// §10: an alternative is one chip, because taking half of it off would
    /// leave `OR` with nothing on one side. Pressing its × takes all of it.
    it('draws an alternative as one chip and takes all of it off at once', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('(#a OR #b) with:khánh');
        await wrapper.find('[data-ask]').trigger('keyup.enter');
        await flushPromises();

        const chips = wrapper.findAll('[data-chip]');
        expect(chips).toHaveLength(2);
        expect(chips[0].attributes('data-chip-kind')).toBe('group');

        await wrapper.findAll('[data-chip-drop]')[0].trigger('click');
        await flushPromises();
        const calls = vi.mocked(invoke).mock.calls;
        expect(calls[calls.length - 1][1]).toMatchObject({ query: 'with:khánh' });
    });

    /// §13.3: a question that pays a model does not get run by the button
    /// everybody presses. It gets a second one, and the first one answers with
    /// the price.
    it('offers a separate door for a question that spends money', async () => {
        const wrapper = mountBar();
        await flushPromises();
        expect(wrapper.find('[data-run-asking]').exists()).toBe(false);

        await wrapper.find('[data-ask]').setValue('is:note | explode sentences | ask 15');
        await flushPromises();
        expect(wrapper.find('[data-run-asking]').exists()).toBe(true);

        // The ordinary button never reaches the paying command.
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();
        expect(vi.mocked(invoke).mock.calls.map(c => c[0])).not.toContain('ask_node_query');

        await wrapper.find('[data-run-asking]').trigger('click');
        await flushPromises();
        expect(vi.mocked(invoke).mock.calls.map(c => c[0])).toContain('ask_node_query');
    });

    /// Two opposite questions used to be one picture: the minus was stripped
    /// off to find the key and then never drawn.
    it('marks a chip that asks for the absence of something', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('-with:khánh');
        await wrapper.find('[data-ask]').trigger('keyup.enter');
        await flushPromises();

        const chip = wrapper.find('[data-chip]');
        expect(chip.attributes('data-chip-not')).toBe('yes');
        expect(chip.find('[data-chip-negated]').exists()).toBe(true);
    });

    /// The same press that put a filter on takes it off, so pressing the same
    /// person twice does not leave a filter nobody can see.
    it('presses off what it pressed on', async () => {
        const wrapper = mountBar();
        await flushPromises();
        const vm = wrapper.vm as unknown as { press: (k: string, v: string) => Promise<void> };
        await vm.press('when', '2019');
        await vm.press('when', '2019');
        await flushPromises();
        expect(wrapper.findAll('[data-chip]')).toHaveLength(0);
    });

    it('goes back to words when asked to edit them', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await (wrapper.vm as unknown as { press: (k: string, v: string) => Promise<void> })
            .press('with', 'Khánh');
        await flushPromises();
        expect(wrapper.find('[data-ask]').exists()).toBe(false);

        await wrapper.find('[data-edit]').trigger('click');
        await flushPromises();
        expect((wrapper.find('[data-ask]').element as HTMLInputElement).value).toBe('with:Khánh');
    });

    // ─── The answer choosing its own shape ──────────────────────

    /// A question with days in it is drawn down the days without anybody
    /// saying so. This is what makes a new question cost no new code.
    it('draws an answer with days in it down the days, unasked', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        expect(wrapper.find('[data-dated-view]').exists()).toBe(true);
        expect(wrapper.find('[data-day-label]').text()).toBe('2019-11-05');
    });

    it('lets one press overrule the answer, and lets it be pressed back', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        const asTable = wrapper.findAll('[data-shape]').find(b => b.text().match(/Bảng|Table/));
        await asTable?.trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-dated-view]').exists()).toBe(false);

        await asTable?.trigger('click');
        await flushPromises();
        // Back to whatever the answer chose for itself.
        expect(wrapper.find('[data-dated-view]').exists()).toBe(true);
    });

    it('marks which shape is in use, however it was chosen', async () => {
        const wrapper = mountBar();
        await flushPromises();
        await wrapper.find('[data-ask]').setValue('with:khánh');
        await wrapper.find('[data-run]').trigger('click');
        await flushPromises();

        const on = wrapper.findAll('[data-shape]').filter(b => b.attributes('data-on') === 'yes');
        expect(on).toHaveLength(1);
        expect(on[0].attributes('aria-pressed')).toBe('true');
    });
});
