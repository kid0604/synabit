import { ref, onMounted, onUnmounted } from 'vue';

/**
 * The current time, refreshed often enough for the "now" line to look alive
 * and rarely enough to cost nothing.
 *
 * A minute would let the line sit visibly stale against the clock in the menu
 * bar; a second would re-render the grid sixty times more often than anyone
 * can see.
 */
export function useNow(intervalMs = 30_000) {
    const now = ref(new Date());
    let timer: ReturnType<typeof setInterval> | null = null;

    onMounted(() => {
        timer = setInterval(() => { now.value = new Date(); }, intervalMs);
    });
    onUnmounted(() => {
        if (timer) clearInterval(timer);
        timer = null;
    });

    return now;
}
