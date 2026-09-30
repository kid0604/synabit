/**
 * The Vietnamese lunisolar calendar (âm lịch), converted to and from the
 * Gregorian one.
 *
 * This is Hồ Ngọc Đức's astronomical algorithm — the one Vietnamese calendar
 * software has used for two decades — rather than a lookup table: a table
 * stops at whatever year somebody typed it out to, and this does not. It
 * works from two things, the instant of each new moon and the sun's
 * longitude, and derives everything else from the rules that define the
 * calendar:
 *
 *  - A lunar month begins on the day (in local time) that holds a new moon.
 *  - Month 11 is the month that contains the winter solstice.
 *  - A year with thirteen months between two month-11s gets a leap month:
 *    the first month that contains no major solar term (no multiple of 30°
 *    of sun longitude) repeats the number of the month before it.
 *
 * The time zone is the only thing that makes this the *Vietnamese* calendar
 * and not the Chinese one. Vietnam has counted in UTC+7 since 1968, China in
 * UTC+8, and when a new moon falls between 23:00 and midnight Hanoi time the
 * two calendars start the month on different days — Tết 1985 and 2007 are
 * the well-known cases. Pass `timeZone` only to compare with another
 * calendar; the default is what a Vietnamese user expects.
 *
 * Pure, no DOM and no locale: the calendar app decides how to write a date
 * down, this only decides which date it is.
 */

/** Vietnam's offset from UTC, in hours. */
export const VIETNAM_TZ = 7;

export interface LunarDate {
    day: number;
    /** 1–12. A leap month carries the number of the month it repeats. */
    month: number;
    /** The lunar year this day belongs to — Tết starts it, not 1 January. */
    year: number;
    /** True in the repeated (nhuận) month. */
    leap: boolean;
}

export interface SolarDate {
    year: number;
    /** 1–12. */
    month: number;
    day: number;
}

const PI = Math.PI;
const INT = Math.floor;

/** The Julian day number of a Gregorian date (Julian before 1582-10-15). */
export const jdFromDate = (dd: number, mm: number, yy: number): number => {
    const a = INT((14 - mm) / 12);
    const y = yy + 4800 - a;
    const m = mm + 12 * a - 3;
    let jd = dd + INT((153 * m + 2) / 5) + 365 * y + INT(y / 4) - INT(y / 100) + INT(y / 400) - 32045;
    if (jd < 2299161) jd = dd + INT((153 * m + 2) / 5) + 365 * y + INT(y / 4) - 32083;
    return jd;
};

/** The Gregorian date of a Julian day number. */
export const jdToDate = (jd: number): SolarDate => {
    let b: number;
    let c: number;
    if (jd > 2299160) {
        const a = jd + 32044;
        b = INT((4 * a + 3) / 146097);
        c = a - INT((b * 146097) / 4);
    } else {
        b = 0;
        c = jd + 32082;
    }
    const d = INT((4 * c + 3) / 1461);
    const e = c - INT((1461 * d) / 4);
    const m = INT((5 * e + 2) / 153);
    return {
        day: e - INT((153 * m + 2) / 5) + 1,
        month: m + 3 - 12 * INT(m / 10),
        year: b * 100 + d - 4800 + INT(m / 10),
    };
};

/**
 * The instant of the k-th new moon after 1900-01-01, as a Julian date (UT).
 * Meeus' mean lunation plus the larger periodic terms; good to a few minutes,
 * which is what deciding the *day* needs.
 */
const newMoon = (k: number): number => {
    const T = k / 1236.85;
    const T2 = T * T;
    const T3 = T2 * T;
    const dr = PI / 180;
    let jd1 = 2415020.75933 + 29.53058868 * k + 0.0001178 * T2 - 0.000000155 * T3;
    jd1 += 0.00033 * Math.sin((166.56 + 132.87 * T - 0.009173 * T2) * dr);
    const M = 359.2242 + 29.10535608 * k - 0.0000333 * T2 - 0.00000347 * T3;
    const Mpr = 306.0253 + 385.81691806 * k + 0.0107306 * T2 + 0.00001236 * T3;
    const F = 21.2964 + 390.67050646 * k - 0.0016528 * T2 - 0.00000239 * T3;
    let c1 = (0.1734 - 0.000393 * T) * Math.sin(M * dr) + 0.0021 * Math.sin(2 * dr * M);
    c1 = c1 - 0.4068 * Math.sin(Mpr * dr) + 0.0161 * Math.sin(dr * 2 * Mpr);
    c1 = c1 - 0.0004 * Math.sin(dr * 3 * Mpr);
    c1 = c1 + 0.0104 * Math.sin(dr * 2 * F) - 0.0051 * Math.sin(dr * (M + Mpr));
    c1 = c1 - 0.0074 * Math.sin(dr * (M - Mpr)) + 0.0004 * Math.sin(dr * (2 * F + M));
    c1 = c1 - 0.0004 * Math.sin(dr * (2 * F - M)) - 0.0006 * Math.sin(dr * (2 * F + Mpr));
    c1 = c1 + 0.001 * Math.sin(dr * (2 * F - Mpr)) + 0.0005 * Math.sin(dr * (2 * Mpr + M));
    const deltaT = T < -11
        ? 0.001 + 0.000839 * T + 0.0002261 * T2 - 0.00000845 * T3 - 0.000000081 * T * T3
        : -0.000278 + 0.000265 * T + 0.000262 * T2;
    return jd1 + c1 - deltaT;
};

/** The sun's apparent longitude at a Julian date, in radians, 0 to 2π. */
const sunLongitude = (jdn: number): number => {
    const T = (jdn - 2451545.0) / 36525;
    const T2 = T * T;
    const dr = PI / 180;
    const M = 357.5291 + 35999.0503 * T - 0.0001559 * T2 - 0.00000048 * T * T2;
    const L0 = 280.46645 + 36000.76983 * T + 0.0003032 * T2;
    let DL = (1.9146 - 0.004817 * T - 0.000014 * T2) * Math.sin(dr * M);
    DL = DL + (0.019993 - 0.000101 * T) * Math.sin(dr * 2 * M) + 0.00029 * Math.sin(dr * 3 * M);
    let L = (L0 + DL) * dr;
    L = L - PI * 2 * INT(L / (PI * 2));
    return L;
};

/** Which 30° sector (0–11) the sun is in at the start of local day `dayNumber`. */
const sunSector = (dayNumber: number, tz: number): number =>
    INT((sunLongitude(dayNumber - 0.5 - tz / 24) / PI) * 6);

/** The local day number on which the k-th new moon falls. */
const newMoonDay = (k: number, tz: number): number => INT(newMoon(k) + 0.5 + tz / 24);

/**
 * Which new moon (as a `k` for `newMoon`) began the month holding local day
 * `dayNumber`.
 *
 * The published version of the algorithm guesses `k` from the mean lunation
 * and tries it and the one after. The mean can run more than a day ahead of
 * the true new moon, and when it does neither guess has started yet and the
 * day comes out as "day 0" — which happens a handful of times between 1900
 * and 2100 (2054-05-07 is one). Stepping back until the month has begun
 * is the same answer everywhere else and the right one there.
 */
const lunationOf = (dayNumber: number, tz: number): number => {
    let k = INT((dayNumber - 2415021.076998695) / 29.530588853) + 1;
    while (newMoonDay(k, tz) > dayNumber) k--;
    return k;
};

/** The day month 11 of the lunar year ending in solar year `yy` begins. */
const lunarMonth11 = (yy: number, tz: number): number => {
    const off = jdFromDate(31, 12, yy) - 2415021;
    const k = INT(off / 29.530588853);
    const nm = newMoonDay(k, tz);
    // Sector 9 starts at 270°, the winter solstice. A new moon already past it
    // began month 12; month 11 is the one before.
    return sunSector(nm, tz) >= 9 ? newMoonDay(k - 1, tz) : nm;
};

/**
 * In a thirteen-month year starting at month 11 on `a11`, how many months
 * after it the leap month is — the first month whose sun sector does not
 * change, i.e. that holds no major solar term.
 */
const leapMonthOffset = (a11: number, tz: number): number => {
    const k = INT((a11 - 2415021.076998695) / 29.530588853 + 0.5);
    let i = 1;
    let arc = sunSector(newMoonDay(k + i, tz), tz);
    let last: number;
    do {
        last = arc;
        i++;
        arc = sunSector(newMoonDay(k + i, tz), tz);
    } while (arc !== last && i < 14);
    return i - 1;
};

/** The lunar date of a Gregorian day. */
export const solarToLunar = (year: number, month: number, day: number, tz: number = VIETNAM_TZ): LunarDate => {
    const dayNumber = jdFromDate(day, month, year);
    const monthStart = newMoonDay(lunationOf(dayNumber, tz), tz);

    let a11 = lunarMonth11(year, tz);
    let b11 = a11;
    let lunarYear: number;
    if (a11 >= monthStart) {
        lunarYear = year;
        a11 = lunarMonth11(year - 1, tz);
    } else {
        lunarYear = year + 1;
        b11 = lunarMonth11(year + 1, tz);
    }

    const lunarDay = dayNumber - monthStart + 1;
    const diff = INT((monthStart - a11) / 29);
    let leap = false;
    let lunarMonth = diff + 11;
    if (b11 - a11 > 365) {
        const leapDiff = leapMonthOffset(a11, tz);
        if (diff >= leapDiff) {
            lunarMonth = diff + 10;
            if (diff === leapDiff) leap = true;
        }
    }
    if (lunarMonth > 12) lunarMonth -= 12;
    if (lunarMonth >= 11 && diff < 4) lunarYear -= 1;
    return { day: lunarDay, month: lunarMonth, year: lunarYear, leap };
};

/**
 * The Gregorian date of a lunar one, or `null` when it does not exist — a
 * leap month asked for in a year whose leap month is another one, or none.
 *
 * Day 30 of a 29-day month is not checked here and comes back as the first
 * of the next month; `lunarMonthLength` is how a caller that cares finds out.
 */
export const lunarToSolar = (
    day: number, month: number, year: number, leap = false, tz: number = VIETNAM_TZ,
): SolarDate | null => {
    let a11: number;
    let b11: number;
    if (month < 11) {
        a11 = lunarMonth11(year - 1, tz);
        b11 = lunarMonth11(year, tz);
    } else {
        a11 = lunarMonth11(year, tz);
        b11 = lunarMonth11(year + 1, tz);
    }
    const k = INT(0.5 + (a11 - 2415021.076998695) / 29.530588853);
    let off = month - 11;
    if (off < 0) off += 12;
    if (b11 - a11 > 365) {
        const leapOff = leapMonthOffset(a11, tz);
        let leapMonth = leapOff - 2;
        if (leapMonth < 0) leapMonth += 12;
        if (leap && month !== leapMonth) return null;
        if (leap || off >= leapOff) off += 1;
    } else if (leap) {
        return null;
    }
    return jdToDate(newMoonDay(k + off, tz) + day - 1);
};

/** 29 or 30: how many days the lunar month holding this Gregorian day has. */
export const lunarMonthLength = (year: number, month: number, day: number, tz: number = VIETNAM_TZ): number => {
    const k = lunationOf(jdFromDate(day, month, year), tz);
    return newMoonDay(k + 1, tz) - newMoonDay(k, tz);
};

export const CAN = ['Giáp', 'Ất', 'Bính', 'Đinh', 'Mậu', 'Kỷ', 'Canh', 'Tân', 'Nhâm', 'Quý'] as const;
export const CHI = ['Tý', 'Sửu', 'Dần', 'Mão', 'Thìn', 'Tỵ', 'Ngọ', 'Mùi', 'Thân', 'Dậu', 'Tuất', 'Hợi'] as const;

/**
 * The year's name in the sixty-year cycle — "Giáp Thìn" for 2024.
 *
 * Written in Vietnamese whatever the interface language: it is a name, the
 * way "Tết" is, and "Wood Dragon" is the Chinese zodiac's translation of a
 * different calendar's cat (Vietnam's Mão is a cat, not a rabbit).
 */
export const canChiYear = (lunarYear: number): string =>
    `${CAN[(((lunarYear + 6) % 10) + 10) % 10]} ${CHI[(((lunarYear + 8) % 12) + 12) % 12]}`;
