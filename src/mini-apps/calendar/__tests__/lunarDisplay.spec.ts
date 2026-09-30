import { describe, it, expect, beforeEach, vi } from 'vitest';

/**
 * The module keeps the stored choice in a module-level ref, so every case
 * gets a fresh copy of it and of the i18n instance whose language it reads.
 */
// Node's own `localStorage` shadows jsdom's and is absent without a file.
const store = new Map<string, string>();
vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => { store.set(k, String(v)); },
    removeItem: (k: string) => { store.delete(k); },
    clear: () => { store.clear(); },
});

const load = async (locale: string, stored?: string) => {
    vi.resetModules();
    localStorage.clear();
    if (stored !== undefined) localStorage.setItem('synabit.calendar.showLunar', stored);
    const { i18n } = await import('../../../i18n');
    i18n.global.locale.value = locale as any;
    return import('../lunarDisplay');
};

const t = (key: string, v?: Record<string, unknown>) =>
    key === 'calendar.lunar_month_leap' ? `${v?.month} nhuận`
        : `Ngày ${v?.day} tháng ${v?.month} năm ${v?.year}`;

describe('lunar display', () => {
    beforeEach(() => { localStorage.clear(); });

    it('is on by default in Vietnamese and off in English', async () => {
        expect((await load('vi')).showLunar.value).toBe(true);
        expect((await load('en')).showLunar.value).toBe(false);
    });

    it('keeps an explicit choice over the language default', async () => {
        const m = await load('vi');
        m.showLunar.value = false;
        expect(localStorage.getItem('synabit.calendar.showLunar')).toBe('0');
        expect((await load('vi', '0')).showLunar.value).toBe(false);
        expect((await load('en', '1')).showLunar.value).toBe(true);
    });

    it('writes the month only on mùng một or when asked', async () => {
        const m = await load('vi');
        expect(m.shortLunar(new Date(2025, 9, 6))).toBe('15');
        expect(m.shortLunar(new Date(2025, 9, 6), true)).toBe('15/8');
        expect(m.shortLunar(new Date(2026, 1, 17))).toBe('1/1');
    });

    it('marks mùng một and rằm', async () => {
        const m = await load('vi');
        expect(m.isNotableLunarDay(new Date(2025, 9, 6))).toBe(true);
        expect(m.isNotableLunarDay(new Date(2026, 1, 17))).toBe(true);
        expect(m.isNotableLunarDay(new Date(2025, 9, 7))).toBe(false);
    });

    it('writes the full date with the year name, and says when a month is leap', async () => {
        const m = await load('vi');
        expect(m.fullLunar(new Date(2025, 9, 6), t)).toBe('Ngày 15 tháng 8 năm Ất Tỵ');
        expect(m.fullLunar(new Date(2025, 6, 25), t)).toBe('Ngày 1 tháng 6 nhuận năm Ất Tỵ');
    });
});
