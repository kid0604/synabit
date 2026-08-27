/**
 * Dates that come round once a year — birthdays, anniversaries.
 *
 * The rules here match `parse_birthday` and `is_anniversary` in
 * `src-tauri/src/calendar/reminders.rs`, which decide when the notification
 * fires. When the two disagree, the People screen shows a countdown to one day
 * and the phone rings on another.
 */

export interface AnnualDate {
    /** 1–12. */
    month: number;
    /** 1–31. */
    day: number;
}

/**
 * The month and day of a yearly date, from either shape the vault holds.
 *
 * `1994-03-02` is what the contact form writes; `03-02` is what somebody types
 * when they know the day but not the year. Both are accepted, and the year is
 * dropped either way — what is wanted is the anniversary, not the birth.
 */
export function parseAnnualDate(raw: string): AnnualDate | null {
    if (!raw) return null;
    const parts = raw.trim().split('-');
    const [rawMonth, rawDay] = parts.length === 3 ? [parts[1], parts[2]]
        : parts.length === 2 ? [parts[0], parts[1]]
        : [undefined, undefined];
    if (rawMonth === undefined || rawDay === undefined) return null;

    const month = Number(rawMonth);
    const day = Number(rawDay);
    if (!Number.isInteger(month) || !Number.isInteger(day)) return null;
    if (month < 1 || month > 12 || day < 1) return null;

    // Checked against a leap year so that 29 February is accepted rather than
    // rejected as the impossible day it is in most years.
    const probe = new Date(2024, month - 1, day);
    if (probe.getMonth() !== month - 1 || probe.getDate() !== day) return null;

    return { month, day };
}

const isLeapYear = (year: number): boolean =>
    new Date(year, 1, 29).getDate() === 29;

/**
 * The next time this date comes round, at midnight local time.
 *
 * Somebody born on 29 February has a birthday in one year out of four; the
 * rest of the time it is kept on the 28th, which is where the reminder engine
 * and the recurring-event engine both put it.
 */
export function nextOccurrence(date: AnnualDate, now: Date = new Date()): Date {
    const dayIn = (year: number): Date => {
        const day = date.month === 2 && date.day === 29 && !isLeapYear(year) ? 28 : date.day;
        return new Date(year, date.month - 1, day);
    };

    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const thisYear = dayIn(now.getFullYear());
    return thisYear >= today ? thisYear : dayIn(now.getFullYear() + 1);
}

/**
 * Whole days until this date comes round again; 0 means today.
 *
 * Returns null for anything that is not a date, so a caller can tell "not set"
 * apart from "today".
 */
export function daysUntilAnnual(raw: string, now: Date = new Date()): number | null {
    const parsed = parseAnnualDate(raw);
    if (!parsed) return null;
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const next = nextOccurrence(parsed, now);
    return Math.round((next.getTime() - today.getTime()) / (1000 * 60 * 60 * 24));
}
