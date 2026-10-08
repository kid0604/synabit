import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import { coloursById } from '../subscriptions';
import type { Subscription, RefreshReport } from '../subscriptions';

/**
 * How often subscribed calendars are re-read while the app is open.
 *
 * Long, and cheaply: every fetch carries the `ETag` the server last gave, so
 * a calendar nobody has touched costs one `304` rather than a download. A
 * shared holiday calendar changes about once a year.
 */
const REFRESH_EVERY_MS = 30 * 60 * 1000;

export function useSubscriptions(onChanged: () => void) {
    const subscriptions = ref<Subscription[]>([]);
    const busy = ref(false);

    const colours = computed(() => coloursById(subscriptions.value));

    // Asked once per screen: a calendar this device has never fetched — one
    // another device subscribed to, or any of them after the cache was
    // rebuilt — would otherwise draw nothing until the next half-hourly round.
    let fetchedUnseen = false;

    const load = async () => {
        try {
            subscriptions.value = await invoke<Subscription[]>('list_calendar_subscriptions');
        } catch (e) {
            logger.error('Could not read the subscribed calendars:', e);
            return;
        }
        if (!fetchedUnseen && subscriptions.value.some(s => s.enabled && s.lastFetchedAt === 0)) {
            fetchedUnseen = true;
            refreshAll().catch(() => {});
        }
    };

    const add = async (url: string, name: string): Promise<RefreshReport | null> => {
        if (!url.trim()) return null;
        busy.value = true;
        try {
            const report = await invoke<RefreshReport>('add_calendar_subscription', {
                url: url.trim(),
                name: name.trim(),
            });
            await load();
            onChanged();
            return report;
        } finally {
            busy.value = false;
        }
    };

    const remove = async (id: string) => {
        await invoke('remove_calendar_subscription', { id });
        await load();
        onChanged();
    };

    const setEnabled = async (id: string, enabled: boolean) => {
        await invoke('set_calendar_subscription_enabled', { id, enabled });
        await load();
        onChanged();
    };

    const setRemind = async (id: string, remind: boolean) => {
        await invoke('set_calendar_subscription_remind', { id, remind });
        await load();
    };

    const rename = async (id: string, name: string) => {
        await invoke('rename_calendar_subscription', { id, name });
        await load();
    };

    const refreshAll = async (): Promise<RefreshReport[]> => {
        busy.value = true;
        try {
            const reports = await invoke<RefreshReport[]>('refresh_calendar_subscriptions');
            await load();
            // Only redraw when something actually came back different — a
            // round of `304`s should not make the grid flicker.
            if (reports.some(r => !r.unchanged && !r.error)) onChanged();
            return reports;
        } finally {
            busy.value = false;
        }
    };

    let timer: ReturnType<typeof setInterval> | null = null;
    onMounted(() => {
        load();
        // Not on open: a fetch racing the first paint delays the calendar for
        // the sake of data that is almost certainly still current.
        timer = setInterval(() => { refreshAll().catch(() => {}); }, REFRESH_EVERY_MS);
    });
    onUnmounted(() => { if (timer) clearInterval(timer); timer = null; });

    return { subscriptions, colours, busy, load, add, remove, setEnabled, setRemind, rename, refreshAll };
}
