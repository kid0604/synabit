import { ref, computed } from 'vue';
import { i18n } from '../../i18n';
import { solarToLunar, canChiYear } from '../../utils/lunar';
import type { LunarDate } from '../../utils/lunar';

/**
 * Whether the calendar shows lunar dates, and how it writes them.
 *
 * The calendar had no settings of its own before this, so the choice lives
 * in `localStorage` beside the other per-device view preferences the mini
 * apps keep (the whiteboard's viewport, the finance rate cache). It is a
 * viewing preference, not vault data: it does not sync, and a device where
 * storage is unavailable simply falls back to the default.
 *
 * The default follows the interface language rather than being written down
 * at first run: a Vietnamese user sees âm lịch without having to find a
 * switch, and somebody who has never touched it and changes language gets
 * the other default. Only an explicit choice is stored, and then it sticks.
 */
const STORAGE_KEY = 'synabit.calendar.showLunar';

const readStored = (): boolean | null => {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        return raw === '1' ? true : raw === '0' ? false : null;
    } catch {
        return null;
    }
};

const stored = ref<boolean | null>(readStored());

/** On for Vietnamese, off for everything else, until somebody decides. */
export const defaultShowLunar = (locale: string): boolean =>
    (locale || '').toLowerCase().split('-')[0] === 'vi';

export const showLunar = computed<boolean>({
    get: () => stored.value ?? defaultShowLunar(i18n.global.locale.value),
    set: (on: boolean) => {
        stored.value = on;
        try {
            localStorage.setItem(STORAGE_KEY, on ? '1' : '0');
        } catch {
            // Private mode or a full quota: the choice lasts for this session.
        }
    },
});

/** The lunar date of a local calendar day. */
export const lunarOf = (date: Date): LunarDate =>
    solarToLunar(date.getFullYear(), date.getMonth() + 1, date.getDate());

/**
 * The few characters a day cell has room for.
 *
 * Just the day, the way a Vietnamese wall calendar prints it, except where a
 * reader needs to know which month they are in: on mùng một, and on the
 * first cell of a view, which otherwise starts mid-month with no month shown.
 */
export const shortLunar = (date: Date, showMonth = false): string => {
    const l = lunarOf(date);
    return l.day === 1 || showMonth ? `${l.day}/${l.month}` : String(l.day);
};

/** Mùng một and rằm — the days people actually look the lunar date up for. */
export const isNotableLunarDay = (date: Date): boolean => {
    const { day } = lunarOf(date);
    return day === 1 || day === 15;
};

type Translate = (key: string, named?: Record<string, unknown>) => string;

/** "Ngày 15 tháng 8 năm Ất Tỵ", with "nhuận" on a leap month. */
export const fullLunar = (date: Date, t: Translate): string => {
    const l = lunarOf(date);
    const month = l.leap
        ? t('calendar.lunar_month_leap', { month: l.month })
        : String(l.month);
    return t('calendar.lunar_full', { day: l.day, month, year: canChiYear(l.year) });
};
