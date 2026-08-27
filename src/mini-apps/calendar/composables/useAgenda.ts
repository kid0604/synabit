import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { EventsInRange } from '../../../types/ipc';
import type { EventMetadata } from '../types';
import { indexOccurrencesByDate, formatDateString, shiftDateString } from '../helpers';
import { localTimeZone } from '../timezone';
import { logger } from '../../../utils/logger';

/**
 * A list of what is coming, and a way to find what has been.
 *
 * The grid answers "what does this week look like". It cannot answer "where
 * was that meeting about the budget" or "when did I last sit down with Anh",
 * and those are the two questions people actually go looking for an old
 * appointment with.
 *
 * The window widens as soon as there is something to search for: an agenda
 * looks forward, but a search is nearly always for something behind you.
 */
const UPCOMING_DAYS = 90;
const SEARCH_BACK_DAYS = 365;
const SEARCH_FORWARD_DAYS = 365;

export interface AgendaDay {
    date: string;
    events: EventMetadata[];
}

export function useAgenda() {
    const query = ref('');
    const personId = ref('');
    const personName = ref('');
    const loading = ref(false);
    const days = ref<AgendaDay[]>([]);

    /** Narrowed by anything at all — which decides how far back to look. */
    const narrowed = computed(() => !!(query.value.trim() || personId.value));

    const range = computed(() => {
        const today = formatDateString(new Date());
        return narrowed.value
            ? { from: shiftDateString(today, -SEARCH_BACK_DAYS), to: shiftDateString(today, SEARCH_FORWARD_DAYS) }
            : { from: today, to: shiftDateString(today, UPCOMING_DAYS) };
    });

    const load = async () => {
        loading.value = true;
        try {
            const { from, to } = range.value;
            const found: EventsInRange = await invoke('search_event_occurrences', {
                query: query.value.trim() || null,
                personId: personId.value || null,
                from,
                to,
                viewerTz: localTimeZone(),
            });
            const byDate = indexOccurrencesByDate(found);
            days.value = [...byDate.entries()]
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([date, events]) => ({
                    date,
                    events: [...events].sort((x, y) =>
                        (x.start_at || '').localeCompare(y.start_at || '')),
                }));
        } catch (e) {
            logger.error('Could not build the agenda:', e);
            days.value = [];
        } finally {
            loading.value = false;
        }
    };

    /** Typing should not send a query per keystroke. */
    let debounce: ReturnType<typeof setTimeout> | null = null;
    watch([query, personId], () => {
        if (debounce) clearTimeout(debounce);
        debounce = setTimeout(load, 250);
    });

    const focusOn = (id: string, name: string) => {
        personId.value = id;
        personName.value = name;
    };
    const clearPerson = () => {
        personId.value = '';
        personName.value = '';
    };

    const total = computed(() => days.value.reduce((n, d) => n + d.events.length, 0));

    return { query, personId, personName, loading, days, total, narrowed, load, focusOn, clearPerson };
}
