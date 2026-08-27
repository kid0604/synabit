import { describe, it, expect } from 'vitest';
import { serializeRRule, parseRRule, defaultRecurrence, weekdayCodeOf } from '../rrule';
import type { RecurrenceFields } from '../rrule';
import contract from '../../../../contracts/rrule.json';

/**
 * The cases live in `contracts/rrule.json` because
 * `src-tauri/src/calendar/rrule.rs` reads the same file. A string this side
 * writes that the other side reads differently would be a series repeating on
 * the wrong days with nothing on screen to show for it.
 */
const cases = contract.cases as { name: string; fields: RecurrenceFields; rrule: string }[];

describe('the recurrence editor and the stored rule agree', () => {
    it('has a contract that actually loaded', () => {
        expect(cases.length).toBeGreaterThanOrEqual(10);
    });

    for (const c of cases) {
        it(`writes "${c.name}"`, () => {
            expect(serializeRRule(c.fields)).toBe(c.rrule);
        });
        it(`reads back "${c.name}"`, () => {
            expect(parseRRule(c.rrule)).toEqual(c.fields);
        });
    }
});

describe('parsing a rule this app did not write', () => {
    /** The same answer Rust gives: no usable FREQ means no rule. */
    it('reads an unusable rule as "does not repeat"', () => {
        for (const bad of ['', '   ', 'INTERVAL=2', 'FREQ=FORTNIGHTLY', 'nonsense']) {
            expect(parseRRule(bad).freq, bad).toBe('none');
        }
    });

    it('skips parts it does not know rather than giving up on the rule', () => {
        const f = parseRRule('FREQ=WEEKLY;BYSETPOS=-1;WKST=SU;X-THING=1');
        expect(f.freq).toBe('weekly');
        expect(f.interval).toBe(1);
    });

    it('takes UNTIL in either shape', () => {
        expect(parseRRule('FREQ=DAILY;UNTIL=20261231').until).toBe('2026-12-31');
        expect(parseRRule('FREQ=DAILY;UNTIL=20261231T235959Z').until).toBe('2026-12-31');
    });

    it('ignores BYDAY on a frequency that does not use it', () => {
        expect(parseRRule('FREQ=MONTHLY;BYDAY=MO').byDay).toEqual([]);
    });

    it('refuses numbers that would make the rule meaningless', () => {
        const f = parseRRule('FREQ=DAILY;INTERVAL=0;COUNT=0');
        expect(f.interval).toBe(1);
        expect(f.endMode).toBe('never');
    });
});

describe('writing a rule the editor got into a strange state', () => {
    const from = (over: Partial<RecurrenceFields>) => ({ ...defaultRecurrence(), ...over });

    it('sorts and deduplicates the chosen days so the string is stable', () => {
        expect(serializeRRule(from({ freq: 'weekly', byDay: ['FR', 'MO', 'FR', 'WE'] })))
            .toBe('FREQ=WEEKLY;BYDAY=MO,WE,FR');
    });

    it('drops a weekday that is not a weekday', () => {
        expect(serializeRRule(from({ freq: 'weekly', byDay: ['MO', 'XX'] })))
            .toBe('FREQ=WEEKLY;BYDAY=MO');
    });

    it('never writes an interval below one', () => {
        expect(serializeRRule(from({ freq: 'daily', interval: 0 }))).toBe('FREQ=DAILY');
        expect(serializeRRule(from({ freq: 'daily', interval: -3 }))).toBe('FREQ=DAILY');
    });

    it('leaves out an end date the user chose but never filled in', () => {
        expect(serializeRRule(from({ freq: 'weekly', endMode: 'until', until: '' })))
            .toBe('FREQ=WEEKLY');
    });
});

describe('weekdayCodeOf', () => {
    it('names the day a date falls on, counting from Monday', () => {
        expect(weekdayCodeOf('2026-03-02')).toBe('MO');
        expect(weekdayCodeOf('2026-03-08')).toBe('SU');
        expect(weekdayCodeOf('2026-03-04T09:30')).toBe('WE');
    });

    it('does not throw on something that is not a date', () => {
        expect(weekdayCodeOf('')).toBe('MO');
    });
});
