import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import YearInYourWords, { type Line } from '../YearInYourWords.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('../../../../utils/logger', () => ({ logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() } }));

const YEAR: Line[] = [
    { day: '2026-01-05', node_id: 'Notes/2026-01-05.md', text: 'Bắt đầu học đàn, ngón tay đau nhưng vui.' },
    { day: '2026-05-14', node_id: 'Notes/2026-05-14.md', text: 'Sáng nay đi bộ với bố, ông kể chuyện năm 54.' },
    { day: '2026-11-20', node_id: 'Notes/2026-11-20.md', text: 'Nộp đơn nghỉ việc. Nhẹ hơn mình tưởng rất nhiều.' },
];

const open = async (lines: Line[]) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'timeline_year') return lines;
        return null;
    });
    const wrapper = mount(YearInYourWords, {
        props: { vaultPath: '/vault' },
        global: { plugins: [i18n] },
    });
    await wrapper.find('button').trigger('click');
    await flushPromises();
    return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('A year in your own words', () => {
    it('shows the sentences and their days, in the order they were written', async () => {
        const wrapper = await open(YEAR);
        const written = wrapper.findAll('[data-year-text]').map((n) => n.text());
        expect(written).toEqual(YEAR.map((l) => l.text));
        expect(wrapper.find('[data-year-line]').text()).toContain('2026-01-05');
    });

    /// §7.6: no opening, no closing, no adjective. Everything on this screen
    /// that is not a date or a control is the person's own writing.
    it('puts no words of its own around them', async () => {
        const wrapper = await open(YEAR);
        const panel = wrapper.find('[data-year-panel]');
        const theirs = YEAR.map((l) => l.text).join('');
        // Strip the person's sentences, the years, the dates and the controls;
        // what is left is everything the app said, and it must be nothing.
        const dropLabel = wrapper.find('[data-year-drop]').text();
        let rest = panel.text();
        for (const line of YEAR) rest = rest.replace(line.text, '').replace(line.day, '');
        rest = rest.split(dropLabel).join('').replace(/\d{4}/g, '').trim();
        expect(rest).toBe('');
        expect(theirs.length).toBeGreaterThan(0);
    });

    it('opens the note a sentence came from', async () => {
        const wrapper = await open(YEAR);
        await wrapper.findAll('[data-year-text]')[1].trigger('click');
        expect(wrapper.emitted('open')?.[0]).toEqual(['Notes/2026-05-14.md', 'note']);
    });

    it('drops a line for good when asked', async () => {
        const wrapper = await open(YEAR);
        await wrapper.findAll('[data-year-drop]')[0].trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_drop_line', {
            vaultPath: '/vault',
            nodeId: 'Notes/2026-01-05.md',
            text: YEAR[0].text,
        });
        expect(wrapper.findAll('[data-year-line]')).toHaveLength(2);
    });

    it('reads another year when one is picked', async () => {
        const wrapper = await open(YEAR);
        const picks = wrapper.findAll('[data-year-pick]');
        await picks[1].trigger('click');
        await flushPromises();
        expect(invoke).toHaveBeenLastCalledWith('timeline_year', {
            vaultPath: '/vault',
            year: Number(picks[1].text()),
        });
    });

    it('says why when the model will not read it, instead of showing nothing', async () => {
        vi.mocked(invoke).mockImplementation(async (command: string) => {
            if (command === 'timeline_year') throw new Error('No model is configured');
            return null;
        });
        const wrapper = mount(YearInYourWords, {
            props: { vaultPath: '/vault' },
            global: { plugins: [i18n] },
        });
        await wrapper.find('button').trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-year-refused]').text()).toContain('No model is configured');
    });
});
