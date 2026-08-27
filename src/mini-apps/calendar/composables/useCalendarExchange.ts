import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { formatDateString } from '../helpers';
import { logger } from '../../../utils/logger';

/**
 * Taking the calendar out, and bringing one in.
 *
 * The reading and writing of the format itself is in Rust — one place, with
 * the exclusive-`DTEND` conversion that a calendar import gets wrong more
 * often than anything else. What is here is the part that has to be here: the
 * file dialogs, and writing each imported event through the same `writeNode`
 * everything else in this app uses, so an event that arrived in a file is
 * indexed, synced and linked exactly like one typed in by hand.
 */
export interface ImportedEvent {
    uid: string;
    title: string;
    is_all_day: boolean;
    start_at: string;
    end_at: string;
    tzid: string;
    rrule: string;
    exceptions: string[];
    location: string;
    description: string;
    tags: string[];
    /**
     * The file said this repeats, in a way this app cannot reproduce
     * faithfully — "the last Friday of the month", say. The event arrives
     * without its repeat rather than with the wrong one.
     */
    rrule_dropped: boolean;
}

export interface ImportSummary {
    added: number;
    updated: number;
    skipped: number;
    /** Arrived with their date but not their repeat. See `rrule_dropped`. */
    lostRepeat: number;
}

export function useCalendarExchange(ns: any) {
    const busy = ref(false);

    /**
     * Write the whole calendar to a file the user chooses.
     *
     * Returns null when they close the dialog — a cancelled save is not a
     * failure, and reporting it as one is how people learn to ignore errors.
     */
    const exportIcs = async (): Promise<number | null> => {
        busy.value = true;
        try {
            // Not `toISOString()`. Before seven in the morning in Hanoi that
            // still reads as yesterday, and a file named for the wrong day is
            // the same mistake this app spent a whole pass removing.
            const stamp = formatDateString(new Date());
            const destination = await save({
                defaultPath: `synabit-calendar-${stamp}.ics`,
                filters: [{ name: 'iCalendar', extensions: ['ics'] }],
            });
            if (!destination) return null;
            return await invoke<number>('export_calendar_ics', { destination });
        } finally {
            busy.value = false;
        }
    };

    const importIcs = async (): Promise<ImportSummary | null> => {
        busy.value = true;
        try {
            const source = await open({
                multiple: false,
                directory: false,
                filters: [{ name: 'iCalendar', extensions: ['ics', 'ical', 'ifb'] }],
            });
            if (!source) return null;

            const events = await invoke<ImportedEvent[]>('read_calendar_ics', {
                source: source as string,
            });
            if (events.length === 0) return { added: 0, updated: 0, skipped: 0, lostRepeat: 0 };

            // An event this vault has seen before is updated where it lives,
            // not left as a second copy beside itself. Matched on the identity
            // that survives a rename rather than on the title.
            const known = await invoke<Record<string, string>>('match_event_uids', {
                uids: events.map(e => e.uid).filter(Boolean),
            });

            const summary: ImportSummary = { added: 0, updated: 0, skipped: 0, lostRepeat: 0 };
            for (const event of events) {
                if (!event.start_at) { summary.skipped++; continue; }
                const existing = event.uid ? known[event.uid] : undefined;
                const relPath = existing || `Events/${crypto.randomUUID()}.md`;
                try {
                    await ns.writeNode({
                        relPath,
                        title: event.title || 'Untitled event',
                        nodeType: 'event',
                        properties: {
                            is_all_day: event.is_all_day,
                            start_at: event.start_at,
                            end_at: event.end_at,
                            location: event.location,
                            tags: event.tags,
                            tzid: event.tzid || null,
                            rrule: event.rrule || null,
                            // The keys an older version of this app wrote. An
                            // imported event carries a rule, so leaving these
                            // would give it two answers about when it repeats.
                            recurrence: null,
                            recurrence_end_at: null,
                            exceptions: event.exceptions,
                            // Carried so a re-import finds this event again
                            // rather than making another copy of it.
                            node_id: event.uid || null,
                        },
                        content: event.description,
                        eventType: existing ? 'updated' : 'created',
                        silent: true,
                    });
                    if (existing) summary.updated++; else summary.added++;
                    if (event.rrule_dropped) summary.lostRepeat++;
                } catch (e) {
                    logger.error('Could not save an imported event:', e);
                    summary.skipped++;
                }
            }
            return summary;
        } finally {
            busy.value = false;
        }
    };

    return { busy, exportIcs, importIcs };
}
