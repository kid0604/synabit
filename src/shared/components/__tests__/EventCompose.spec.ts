import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import EventCompose, { type ComposedReply } from '../EventCompose.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const lunch: ComposedReply = {
    read: {
        title: 'ăn trưa',
        happened_from: '2026-09-16',
        happened_to: '2026-09-16',
        precision: 'day',
        dated: true,
        with: [{ name: 'Khánh', node_id: 'People/khanh.md' }],
        place: null,
    },
    model: 'qwen3:8b',
    refused: null,
};

const open = async (replies: ComposedReply[], written = 'Notes/day.md') => {
    let at = 0;
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        if (command === 'timeline_read_line') return replies[Math.min(at++, replies.length - 1)];
        if (command === 'timeline_write_event') return written;
        return null;
    });
    const wrapper = mount(EventCompose, {
        props: { vaultPath: '/vault', format: 'YYYY-MM-DD', tag: 'daily' },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    await wrapper.find('[data-compose-open]').trigger('click');
    return wrapper;
};

const noModel: ComposedReply = { read: null, model: null, refused: null };

describe('EventCompose', () => {
    beforeEach(() => vi.mocked(invoke).mockReset());

    it('asks for the fields itself when no model is set up', async () => {
        const wrapper = await open([noModel]);
        expect(wrapper.find('[data-no-model]').exists()).toBe(true);
        expect(wrapper.find('[data-line]').exists()).toBe(false);
        expect(wrapper.find('[data-field-title]').exists()).toBe(true);
    });

    it('lets the model fill the form in', async () => {
        const wrapper = await open([{ read: null, model: 'qwen3:8b', refused: null }, lunch]);
        expect(wrapper.find('[data-line]').exists()).toBe(true);

        await wrapper.find('[data-line]').setValue('hôm qua ăn trưa với Khánh');
        await wrapper.find('[data-read]').trigger('click');
        await flushPromises();

        expect((wrapper.find('[data-field-title]').element as HTMLInputElement).value).toBe('ăn trưa');
        expect((wrapper.find('[data-field-from]').element as HTMLInputElement).value).toBe('2026-09-16');
        expect((wrapper.find('[data-field-people]').element as HTMLInputElement).value).toBe('People/khanh.md');
    });

    /// The whole reason the form is editable: a reading nobody can correct is
    /// a reading that puts strangers in your vault.
    it('writes what the person left in the fields, not what the model said', async () => {
        const wrapper = await open([{ read: null, model: 'qwen3:8b', refused: null }, lunch]);
        await wrapper.find('[data-line]').setValue('hôm qua ăn trưa với Khánh');
        await wrapper.find('[data-read]').trigger('click');
        await flushPromises();

        await wrapper.find('[data-field-title]').setValue('ăn trưa với sếp');
        await wrapper.find('[data-field-people]').setValue('People/khanh.md, Hải');
        await wrapper.find('[data-field-where]').setValue('quán cũ');
        await wrapper.find('[data-write]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_write_event', {
            vaultPath: '/vault',
            title: 'ăn trưa với sếp',
            happenedFrom: '2026-09-16',
            happenedTo: '2026-09-16',
            precision: 'day',
            with: ['People/khanh.md', 'Hải'],
            place: 'quán cũ',
            formatStr: 'YYYY-MM-DD',
            tag: 'daily',
        });
        expect(wrapper.find('[data-saved]').text()).toContain('day.md');
        expect(wrapper.emitted('changed')).toHaveLength(1);
    });

    it('says why the model gave nothing back, and still lets the person write it', async () => {
        const refused: ComposedReply = { read: null, model: 'qwen3:8b', refused: 'Nothing in that line reads as something that happened' };
        const wrapper = await open([{ read: null, model: 'qwen3:8b', refused: null }, refused]);
        await wrapper.find('[data-line]').setValue('mai đi họp');
        await wrapper.find('[data-read]').trigger('click');
        await flushPromises();

        expect(wrapper.find('[data-note]').text()).toContain('Nothing in that line');
        expect(wrapper.find('[data-write]').attributes('disabled')).toBeDefined();
        await wrapper.find('[data-field-title]').setValue('đi họp');
        expect(wrapper.find('[data-write]').attributes('disabled')).toBeUndefined();
    });
});
