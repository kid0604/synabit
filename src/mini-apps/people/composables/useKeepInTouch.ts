import { logger } from '../../../utils/logger';
import { cadenceDays } from './useRelationshipHealth';

/**
 * Answering a nudge in one move.
 *
 * A reminder that takes three clicks to resolve — open the app, find the
 * person, open the timeline, log something — is a reminder people learn to
 * dismiss. These are the two answers anybody actually has when told they have
 * not spoken to somebody in a while: *I have* and *not yet*.
 *
 * # Where the buttons are
 *
 * In the app, beside the person in the reminders list. Not on the notification
 * itself: `tauri-plugin-notification` only offers action buttons on iOS and
 * Android, and there is no way to try that here. Half a mechanism, shipped
 * untested, would be worse than none — the desktop half works everywhere and
 * the phone half is written down as still to do.
 */

/** Today, as the vault writes a date. Local, not UTC. */
export function today(now: Date = new Date()): string {
    return [
        now.getFullYear(),
        String(now.getMonth() + 1).padStart(2, '0'),
        String(now.getDate()).padStart(2, '0'),
    ].join('-');
}

/**
 * The date to count the next nudge from, to push one off by `days`.
 *
 * Snoozing does not silence the cadence, it moves the clock: putting a weekly
 * nudge off for a week means counting from today rather than from whenever
 * they were last spoken to. There is nothing to store beyond that, which is
 * why a snooze survives sync without a field of its own.
 */
export function snoozedFrom(person: any, days: number, now: Date = new Date()): string {
    const cadence = cadenceDays(person);
    if (cadence === null) return today(now);
    // Due again in `days`: count from `cadence - days` ago.
    const from = new Date(now);
    from.setDate(from.getDate() - (cadence - days));
    return today(from);
}

export function useKeepInTouch(ns: any) {
    const write = async (person: any, last_contacted: string) => {
        await ns.writeNode({
            relPath: person.id,
            title: person.title,
            nodeType: 'person',
            properties: { last_contacted },
        });
    };

    /** "I spoke to them." The cadence starts again from today. */
    const markContacted = async (person: any) => {
        try {
            await write(person, today());
            return true;
        } catch (e) {
            logger.error(`Failed to record contact with ${person?.title}`, e);
            return false;
        }
    };

    /** "Not yet." Ask again in `days`, default a week. */
    const snooze = async (person: any, days = 7) => {
        try {
            await write(person, snoozedFrom(person, days));
            return true;
        } catch (e) {
            logger.error(`Failed to snooze ${person?.title}`, e);
            return false;
        }
    };

    return { markContacted, snooze };
}
