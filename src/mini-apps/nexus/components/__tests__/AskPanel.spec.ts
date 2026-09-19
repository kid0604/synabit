import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import AskPanel, { type Question } from '../AskPanel.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const SYNABIT: Question = {
    about: 'uuid-syn',
    name: 'Synabit 1.0',
    node_type: 'project',
    times: 30,
    first: '2026-04-02',
    last: '2026-07-18',
    quiet_for: 428,
};

const open = async (question: Question | null) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'timeline_ask') return question;
        if (command === 'timeline_write_event') return 'Notes/2026-07-18.md';
        return null;
    });
    const wrapper = mount(AskPanel, {
        props: { vaultPath: '/vault', format: 'YYYY-MM-DD', tag: 'daily' },
        global: { plugins: [i18n] },
    });
    await wrapper.find('button').trigger('click');
    await flushPromises();
    return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('What happened to…', () => {
    it('states what it counted, then asks', async () => {
        const wrapper = await open(SYNABIT);
        expect(wrapper.find('[data-ask-counted]').text()).toContain('Synabit 1.0');
        expect(wrapper.find('[data-ask-counted]').text()).toContain('30');
        expect(wrapper.find('[data-ask-question]').text()).toMatch(/\?$/);
    });

    /// §7.4: never call it unfinished, and never conclude anything.
    it('never calls it unfinished and never concludes', async () => {
        const wrapper = await open(SYNABIT);
        const said = wrapper.find('[data-ask-panel]').text().toLowerCase();
        for (const verdict of ['dở dang', 'dang dở', 'unfinished', 'bỏ dở', 'thất bại', 'abandoned']) {
            expect(said).not.toContain(verdict);
        }
        expect(said).not.toContain('!');
    });

    it('asks only when the panel is opened, because asking is a write', async () => {
        vi.mocked(invoke).mockResolvedValue(SYNABIT);
        const wrapper = mount(AskPanel, {
            props: { vaultPath: '/vault', format: 'YYYY-MM-DD', tag: 'daily' },
            global: { plugins: [i18n] },
        });
        await flushPromises();
        expect(invoke).not.toHaveBeenCalled();

        await wrapper.find('button').trigger('click');
        await flushPromises();
        expect(invoke).toHaveBeenCalledWith('timeline_ask', { vaultPath: '/vault' });
    });

    it('writes the answer as a real event, linked to the thing', async () => {
        const wrapper = await open(SYNABIT);
        await wrapper.find('[data-ask-answer]').setValue('Dừng lại để làm cái khác.');
        await wrapper.find('[data-ask-save]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_write_event', {
            vaultPath: '/vault',
            title: 'Dừng lại để làm cái khác.',
            happenedFrom: '2026-07-18',
            happenedTo: '2026-07-18',
            precision: 'day',
            with: [],
            place: null,
            about: ['uuid-syn'],
            formatStr: 'YYYY-MM-DD',
            tag: 'daily',
        });
        expect(wrapper.emitted('changed')).toHaveLength(1);
    });

    it('puts a person in the cast rather than the subject', async () => {
        const wrapper = await open({ ...SYNABIT, node_type: 'person', about: 'uuid-khanh' });
        await wrapper.find('[data-ask-answer]').setValue('Nó chuyển vào Sài Gòn.');
        await wrapper.find('[data-ask-save]').trigger('click');
        await flushPromises();

        const written = vi.mocked(invoke).mock.calls.find((c) => c[0] === 'timeline_write_event');
        expect(written?.[1]).toMatchObject({ with: ['uuid-khanh'], about: [] });
    });

    it('costs nothing to walk away from', async () => {
        const wrapper = await open(SYNABIT);
        await wrapper.find('[data-ask-skip]').trigger('click');
        await flushPromises();
        expect(wrapper.find('[data-ask-panel]').exists()).toBe(false);
        expect(vi.mocked(invoke).mock.calls.some((c) => c[0] === 'timeline_write_event')).toBe(false);
    });

    it('says so plainly when there is nothing to ask about', async () => {
        const wrapper = await open(null);
        expect(wrapper.find('[data-ask-panel]').text()).toBeTruthy();
        expect(wrapper.find('[data-ask-counted]').exists()).toBe(false);
    });
});
