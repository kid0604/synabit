import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import SilencePanel, { type Absent } from '../SilencePanel.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const KHANH: Absent = {
    who: 'uuid-khanh',
    name: 'Khánh',
    times: 20,
    first: '2019-01-12',
    last: '2021-03-14',
    quiet_for: 640,
    longest_before: 58,
};

const open = async (absent: Absent[]) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'timeline_silences') return absent;
        return null;
    });
    const wrapper = mount(SilencePanel, {
        props: { vaultPath: '/vault' },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    await wrapper.find('button').trigger('click');
    await flushPromises();
    return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('Gone quiet', () => {
    it('states what it counted and nothing else', async () => {
        const wrapper = await open([KHANH]);
        const said = wrapper.find('[data-absent]').text();
        expect(said).toContain('Khánh');
        expect(said).toContain('20');
        expect(said).toContain('2021-03-14');
    });

    /// §7.2's two forbidden moves: advising contact, and guessing a reason.
    it('never suggests getting in touch and never offers a reason', async () => {
        const wrapper = await open([KHANH]);
        const said = wrapper.find('[data-silence-panel]').text().toLowerCase();
        for (const forbidden of ['nhắn', 'gọi', 'liên lạc', 'có lẽ', 'vì sao', 'tại sao', 'hãy']) {
            expect(said).not.toContain(forbidden);
        }
        expect(said).not.toContain('!');
    });

    it('takes one click to leave somebody out of it', async () => {
        const wrapper = await open([KHANH]);
        await wrapper.find('[data-set-aside]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_set_aside', {
            vaultPath: '/vault',
            who: 'uuid-khanh',
        });
        expect(wrapper.find('[data-absent]').exists()).toBe(false);
    });

    it('says nothing when no pattern is clear enough', async () => {
        const wrapper = await open([]);
        expect(wrapper.find('[data-silence-count]').exists()).toBe(false);
        expect(wrapper.find('button').attributes('disabled')).toBeDefined();
    });
});
