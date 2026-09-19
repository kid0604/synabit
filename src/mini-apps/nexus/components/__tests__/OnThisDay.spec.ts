import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import OnThisDay, { type Looking } from '../OnThisDay.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const WALK: Looking = {
    day: '2020-05-14',
    years_ago: 6,
    title: 'Đi bộ với bố',
    quote: 'Sáng nay đi bộ với bố, ông kể chuyện năm 54.',
    node_id: 'Notes/2020-05-14.md',
    kind: 'note',
};

const open = async (looking: Looking[]) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'timeline_on_this_day') return looking;
        return null;
    });
    const wrapper = mount(OnThisDay, {
        props: { vaultPath: '/vault', atDate: '2026-05-14' },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    await wrapper.find('button').trigger('click');
    await flushPromises();
    return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('On this day', () => {
    it('hands back the sentence as it was written, and nothing over it', async () => {
        const wrapper = await open([WALK]);
        const quote = wrapper.find('[data-quote]');
        expect(quote.text()).toContain(WALK.quote);
        // The panel may frame the sentence, but it may not write one of its own.
        const panel = wrapper.find('[data-onthisday-panel]').text();
        expect(panel).not.toMatch(/[!❤️🎉]/);
        expect(panel).toContain('2020-05-14');
    });

    it('opens the note the sentence came from', async () => {
        const wrapper = await open([WALK]);
        await wrapper.find('[data-quote]').trigger('click');
        expect(wrapper.emitted('open')?.[0]).toEqual(['Notes/2020-05-14.md', 'note']);
    });

    it('takes one click to refuse, and asks for that to be remembered', async () => {
        const wrapper = await open([WALK]);
        await wrapper.find('[data-not-again]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_not_again', {
            vaultPath: '/vault',
            nodeId: 'Notes/2020-05-14.md',
            day: '2020-05-14',
        });
        expect(wrapper.find('[data-looking]').exists()).toBe(false);
    });

    it('says nothing at all when there is nothing to quote', async () => {
        const wrapper = await open([]);
        expect(wrapper.find('[data-looking-count]').exists()).toBe(false);
        expect(wrapper.find('button').attributes('disabled')).toBeDefined();
    });

    it('follows the strip to another day', async () => {
        const wrapper = await open([WALK]);
        vi.mocked(invoke).mockResolvedValue([]);
        await wrapper.setProps({ atDate: '2026-03-01' });
        await flushPromises();
        expect(invoke).toHaveBeenLastCalledWith('timeline_on_this_day', {
            vaultPath: '/vault',
            day: '2026-03-01',
        });
        expect(wrapper.find('[data-looking]').exists()).toBe(false);
    });
});
