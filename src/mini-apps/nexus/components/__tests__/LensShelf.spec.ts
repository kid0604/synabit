import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import LensShelf from '../LensShelf.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../../utils/logger', () => ({
    logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

const writeNode = vi.fn().mockResolvedValue(undefined);
const trashNode = vi.fn().mockResolvedValue('');
vi.mock('../../../../composables/useNodeService', () => ({
    useNodeService: () => ({ writeNode, trashNode }),
}));

const SHELF = {
    columns: ['title', 'query', 'render', 'icon'],
    rows: [
        {
            id: 'Lens/khanh.md',
            node_type: 'lens',
            title: 'Lens/khanh.md',
            cells: ['Gặp Khánh', 'with:khánh when:2019/2026', 'strip', ''],
        },
        {
            id: 'Lens/food.md',
            node_type: 'lens',
            title: 'Lens/food.md',
            cells: ['Ăn ở đâu', '#ăn | count by where', 'bars', ''],
        },
    ],
    total: 2,
    query_time_ms: 1,
};

const open = async (query = '', active: string | null = null) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'run_node_query') return structuredClone(SHELF);
        return null;
    });
    const wrapper = mount(LensShelf, {
        props: { vaultPath: '/vault', query, active },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    return wrapper;
};

beforeEach(() => {
    vi.mocked(invoke).mockReset();
    writeNode.mockClear();
    trashNode.mockClear();
});

describe('The lens shelf', () => {
    it('is read with an ordinary query, not a command of its own', async () => {
        await open();
        expect(invoke).toHaveBeenCalledWith('run_node_query', {
            vaultPath: '/vault',
            query: expect.stringContaining('is:lens'),
            offset: 0,
        });
    });

    it('shows each saved question by its name, with the question itself to hover', async () => {
        const wrapper = await open();
        const lenses = wrapper.findAll('[data-lens]');
        expect(lenses.map(l => l.text())).toEqual(['Gặp Khánh', 'Ăn ở đâu']);
        expect(lenses[0].attributes('title')).toBe('with:khánh when:2019/2026');
    });

    it('hands the whole lens back when one is pressed', async () => {
        const wrapper = await open();
        await wrapper.findAll('[data-lens]')[1].trigger('click');
        expect(wrapper.emitted('run')?.[0][0]).toMatchObject({
            id: 'Lens/food.md',
            query: '#ăn | count by where',
            render: 'bars',
        });
    });

    /// Saving sits beside the answer: the moment somebody wants to keep a
    /// question is the moment they have just seen it answer correctly.
    it('offers to keep the question on the bar, named after it by default', async () => {
        const wrapper = await open('with:minh when:2026');
        await wrapper.find('[data-lens-save]').trigger('click');
        expect((wrapper.find('[data-lens-name]').element as HTMLInputElement).value).toBe(
            'with:minh when:2026',
        );

        await wrapper.find('[data-lens-name]').setValue('Gặp Minh');
        await wrapper.find('[data-lens-keep]').trigger('click');
        await flushPromises();

        expect(writeNode).toHaveBeenCalledWith(
            expect.objectContaining({
                nodeType: 'lens',
                title: 'Gặp Minh',
                properties: { query: 'with:minh when:2026' },
            }),
        );
        expect(writeNode.mock.calls[0][0].relPath).toMatch(/^Lens\/.+\.md$/);
        expect(wrapper.emitted('changed')).toHaveLength(1);
    });

    it('does not offer to save a question that is already on the shelf', async () => {
        const wrapper = await open('with:khánh when:2019/2026');
        expect(wrapper.find('[data-lens-save]').exists()).toBe(false);
    });

    it('does not offer to save nothing', async () => {
        const wrapper = await open('   ');
        expect(wrapper.find('[data-lens-save]').exists()).toBe(false);
    });

    it('puts a lens away when asked, and it leaves the shelf', async () => {
        const wrapper = await open();
        await wrapper.findAll('[data-forget]')[0].trigger('click');
        await flushPromises();

        expect(trashNode).toHaveBeenCalledWith({ relPath: 'Lens/khanh.md' });
        expect(wrapper.findAll('[data-lens]').map(l => l.text())).toEqual(['Ăn ở đâu']);
    });

    /// Both actions are real buttons, side by side. A button nested inside a
    /// button is invalid markup and the inner one never takes focus.
    it('lets the keyboard reach putting a lens away', async () => {
        const wrapper = await open();
        const forget = wrapper.findAll('[data-forget]')[0];
        expect(forget.element.tagName).toBe('BUTTON');
        expect(forget.attributes('tabindex')).toBeUndefined();
        expect(forget.attributes('aria-label')).toBeTruthy();
    });

    it('says so plainly when nothing has been saved', async () => {
        vi.mocked(invoke).mockImplementation(async () => ({
            columns: [],
            rows: [],
            total: 0,
            query_time_ms: 0,
        }));
        const wrapper = mount(LensShelf, {
            props: { vaultPath: '/vault', query: '', active: null },
            global: { plugins: [i18n] },
        });
        await flushPromises();
        expect(wrapper.find('[data-lens-empty]').exists()).toBe(true);
    });
});
