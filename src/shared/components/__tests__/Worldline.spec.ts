import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import Worldline, { type Presence } from '../Worldline.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const show = async (stretches: Presence[]) => {
    vi.mocked(invoke).mockResolvedValue(stretches);
    const wrapper = mount(Worldline, {
        props: { vaultPath: '/vault', nodeId: 'People/khanh.md' },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    return wrapper;
};

describe('Worldline', () => {
    beforeEach(() => vi.mocked(invoke).mockReset());

    it('shows a stretch for each time they were around', async () => {
        const wrapper = await show([
            { from_day: '2015-03-01', to_day: '2015-06-01', events: 2 },
            { from_day: '2019-08-02', to_day: '2021-01-04', events: 20 },
        ]);
        const found = wrapper.findAll('[data-stretch]');
        expect(found).toHaveLength(2);
        expect(found[0].text()).toContain('2015');
        expect(found[1].text()).toContain('2019 – 2021');
        expect(found[1].text()).toContain('20');
    });

    it('leaves a span still running open at the far end', async () => {
        const wrapper = await show([{ from_day: '2021-03-01', to_day: '9999-12-31', events: 4 }]);
        expect(wrapper.find('[data-stretch]').text()).toContain('2021 →');
    });

    it('says nothing is known rather than drawing an empty year', async () => {
        const wrapper = await show([]);
        expect(wrapper.find('[data-worldline-empty]').exists()).toBe(true);
        expect(wrapper.findAll('[data-stretch]')).toHaveLength(0);
    });

    it('asks nothing when there is no node to ask about', async () => {
        const wrapper = mount(Worldline, {
            props: { vaultPath: '/vault', nodeId: '' },
            global: { plugins: [i18n] },
        });
        await flushPromises();
        expect(invoke).not.toHaveBeenCalled();
        expect(wrapper.find('[data-worldline-empty]').exists()).toBe(true);
    });
});
