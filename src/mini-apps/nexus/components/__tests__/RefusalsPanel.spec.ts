import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import RefusalsPanel, { type Hushed, type Pin } from '../RefusalsPanel.vue';
import { i18n } from '../../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const HUSHED: Hushed = {
    hushes: [
        { id: 'h1', about: 'person', who: 'People/khanh.md', until: '2027-03-19', made: '2026-09-19' },
        {
            id: 'h2',
            about: 'moment',
            node: 'Notes/2026-05-14.md',
            day: '2026-05-14',
            until: null,
            made: '2026-09-18',
        },
    ],
    dead: ['People/ba.md'],
};

const PINS: Pin[] = [{ id: 'p1', node: 'Notes/2026-03-02.md', day: '2026-03-02' }];

const open = async (hushed: Hushed, pins: Pin[]) => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
        // A fresh copy each call: the panel assigns over what it was given,
        // and a shared literal would leak one test's undo into the next.
        if (command === 'timeline_quiet') return structuredClone(hushed);
        if (command === 'timeline_pins') return structuredClone(pins);
        return null;
    });
    const wrapper = mount(RefusalsPanel, {
        props: { vaultPath: '/vault' },
        global: { plugins: [i18n] },
    });
    await flushPromises();
    await wrapper.find('button').trigger('click');
    await flushPromises();
    return wrapper;
};

beforeEach(() => vi.mocked(invoke).mockReset());

describe('What you refused', () => {
    it('lists every refusal with what it covers and how long it holds', async () => {
        const wrapper = await open(HUSHED, PINS);
        const rows = wrapper.findAll('[data-hush]').map((n) => n.text());
        expect(rows[0]).toContain('khanh');
        expect(rows[0]).toContain('2027-03-19');
        expect(rows[1]).toContain('2026-05-14');
        expect(wrapper.find('[data-pin]').text()).toContain('2026-03-02');
        expect(wrapper.find('[data-refusal-count]').text()).toBe('3');
    });

    /// The gap this panel exists to close: before it, a tap could be made but
    /// never taken back except by deleting a file in the vault by hand.
    it('takes a hush back in one click', async () => {
        const wrapper = await open(HUSHED, PINS);
        await wrapper.findAll('[data-undo]')[0].trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_unhush', { vaultPath: '/vault', id: 'h1' });
        expect(wrapper.findAll('[data-hush]')).toHaveLength(1);
        expect(wrapper.emitted('changed')).toHaveLength(1);
    });

    it('takes a pin back in one click', async () => {
        const wrapper = await open(HUSHED, PINS);
        await wrapper.find('[data-unpin]').trigger('click');
        await flushPromises();

        expect(invoke).toHaveBeenCalledWith('timeline_unpin', { vaultPath: '/vault', id: 'p1' });
        expect(wrapper.find('[data-pin]').exists()).toBe(false);
    });

    /// §7.3. Nobody decided this, so there is nothing here to undo — and
    /// offering to would be the cruellest thing this screen could do.
    it('lists the dead apart, with no way to undo them', async () => {
        const wrapper = await open(HUSHED, PINS);
        const dead = wrapper.find('[data-dead]');
        expect(dead.text()).toContain('ba');
        expect(dead.find('[data-undo]').exists()).toBe(false);
        // They are counted as refusals by nobody, so the badge ignores them.
        expect(wrapper.find('[data-refusal-count]').text()).toBe('3');
    });

    it('says so plainly when nothing has been refused', async () => {
        const wrapper = await open({ hushes: [], dead: [] }, []);
        expect(wrapper.find('[data-refusal-count]').exists()).toBe(false);
        expect(wrapper.find('[data-hush]').exists()).toBe(false);
        expect(wrapper.find('[data-refusals-panel]').text()).toBeTruthy();
    });
});
