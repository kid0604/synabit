import { describe, it, expect } from 'vitest';
import {
    solarToLunar, lunarToSolar, lunarMonthLength, canChiYear, jdFromDate, jdToDate,
} from '../lunar';

/**
 * The fixed points are dates a Vietnamese reader can check against any wall
 * calendar. The ones from 1968, 1985 and 2007 are there because the Chinese
 * calendar (UTC+8) disagrees with the Vietnamese one on them — a conversion
 * that got the time zone wrong would still pass every other case.
 */
describe('solarToLunar', () => {
    const cases: [string, [number, number, number], { day: number; month: number; year: number; leap: boolean }][] = [
        ['Tết Giáp Thìn 2024', [2024, 2, 10], { day: 1, month: 1, year: 2024, leap: false }],
        ['the last day of Quý Mão', [2024, 2, 9], { day: 30, month: 12, year: 2023, leap: false }],
        ['Tết Ất Tỵ 2025', [2025, 1, 29], { day: 1, month: 1, year: 2025, leap: false }],
        ['Tết Bính Ngọ 2026', [2026, 2, 17], { day: 1, month: 1, year: 2026, leap: false }],
        ['month 2 of 2023', [2023, 2, 20], { day: 1, month: 2, year: 2023, leap: false }],
        ['leap month 2 of 2023', [2023, 3, 22], { day: 1, month: 2, year: 2023, leap: true }],
        ['month 6 of 2025', [2025, 6, 25], { day: 1, month: 6, year: 2025, leap: false }],
        ['leap month 6 of 2025', [2025, 7, 25], { day: 1, month: 6, year: 2025, leap: true }],
        ['Tết Trung thu 2025', [2025, 10, 6], { day: 15, month: 8, year: 2025, leap: false }],
        ['Tết Mậu Thân 1968 (China: a day later)', [1968, 1, 29], { day: 1, month: 1, year: 1968, leap: false }],
        ['Tết Ất Sửu 1985 (China: a month later)', [1985, 1, 21], { day: 1, month: 1, year: 1985, leap: false }],
        ['Tết Đinh Hợi 2007 (China: a day later)', [2007, 2, 17], { day: 1, month: 1, year: 2007, leap: false }],
    ];

    it.each(cases)('%s', (_name, [y, m, d], expected) => {
        expect(solarToLunar(y, m, d)).toEqual(expected);
    });

    it('is the Chinese calendar when asked in UTC+8, which is how the cases above differ', () => {
        expect(solarToLunar(1985, 2, 20, 8)).toMatchObject({ day: 1, month: 1, year: 1985 });
        expect(solarToLunar(2007, 2, 18, 8)).toMatchObject({ day: 1, month: 1, year: 2007 });
    });
});

describe('lunarToSolar', () => {
    it('finds Tết and the leap months', () => {
        expect(lunarToSolar(1, 1, 2026)).toEqual({ year: 2026, month: 2, day: 17 });
        expect(lunarToSolar(1, 2, 2023, true)).toEqual({ year: 2023, month: 3, day: 22 });
        expect(lunarToSolar(1, 6, 2025, true)).toEqual({ year: 2025, month: 7, day: 25 });
        expect(lunarToSolar(15, 8, 2025)).toEqual({ year: 2025, month: 10, day: 6 });
    });

    it('refuses a leap month the year does not have', () => {
        expect(lunarToSolar(1, 6, 2024, true)).toBeNull();
        expect(lunarToSolar(1, 3, 2023, true)).toBeNull();
    });
});

describe('canChiYear', () => {
    it('names the year in the sixty-year cycle', () => {
        expect(canChiYear(2024)).toBe('Giáp Thìn');
        expect(canChiYear(2025)).toBe('Ất Tỵ');
        expect(canChiYear(2026)).toBe('Bính Ngọ');
        expect(canChiYear(1984)).toBe('Giáp Tý');
        expect(canChiYear(2023)).toBe('Quý Mão');
    });
});

/**
 * Every day from 1900 to 2100, walked in order. Spot checks prove the
 * formula on a dozen days; this proves the shape of the whole thing — no
 * skipped or repeated day, no month of 28 or 31, one leap month at most a
 * year and month numbers that only ever move forward.
 */
describe('1900–2100, day by day', () => {
    it('is continuous, round-trips and keeps the calendar\'s shape', () => {
        const start = jdFromDate(1, 1, 1900);
        const end = jdFromDate(31, 12, 2100);
        let prev = (() => { const s = jdToDate(start); return solarToLunar(s.year, s.month, s.day); })();
        let monthLen = 0;
        let leapsThisYear = 0;
        const failures: string[] = [];

        for (let jd = start + 1; jd <= end && failures.length < 5; jd++) {
            const s = jdToDate(jd);
            const l = solarToLunar(s.year, s.month, s.day);
            const where = `${s.year}-${s.month}-${s.day}`;

            if (l.day === prev.day + 1) {
                if (l.month !== prev.month || l.year !== prev.year || l.leap !== prev.leap) failures.push(`${where}: month changed mid-month`);
            } else if (l.day === 1) {
                const len = prev.day;
                if (monthLen && (len < 29 || len > 30)) failures.push(`${where}: a month of ${len} days`);
                monthLen = len;
                if (l.leap) {
                    leapsThisYear++;
                    if (l.month !== prev.month || prev.leap) failures.push(`${where}: leap month does not repeat the one before`);
                } else if (l.month === 1) {
                    if (prev.month !== 12 || l.year !== prev.year + 1) failures.push(`${where}: Tết not after month 12`);
                    if (leapsThisYear > 1) failures.push(`${where}: ${leapsThisYear} leap months in a year`);
                    leapsThisYear = 0;
                } else if (l.month !== prev.month + 1 || l.year !== prev.year) {
                    failures.push(`${where}: month ${prev.month} followed by ${l.month}`);
                }
                if (lunarMonthLength(s.year, s.month, s.day) < 29) failures.push(`${where}: length`);
            } else {
                failures.push(`${where}: day ${prev.day} followed by ${l.day}`);
            }

            const back = lunarToSolar(l.day, l.month, l.year, l.leap);
            if (!back || back.year !== s.year || back.month !== s.month || back.day !== s.day) {
                failures.push(`${where}: round trip gave ${JSON.stringify(back)}`);
            }
            prev = l;
        }
        expect(failures).toEqual([]);
    }, 60_000);

    it('reports 29 or 30 for the month a day is in', () => {
        expect(lunarMonthLength(2024, 2, 9)).toBe(30); // 30 tháng Chạp Quý Mão exists
        expect([29, 30]).toContain(lunarMonthLength(2025, 10, 6));
    });
});
