import { ref, watch, type Ref } from 'vue';

/**
 * Moving through a list without a mouse.
 *
 * The People sidebar is a list of buttons, so Tab already reaches every one of
 * them — which is the problem: with two thousand contacts, Tab is not a way
 * through a list, it is a way to lose an afternoon. This makes the list one
 * stop, and the arrows move within it, which is what the ARIA listbox pattern
 * is for and what people already expect from every other list they use.
 *
 * # What it deliberately does not do
 *
 * It does not scroll. The active row is given `tabindex="0"` and focused, and
 * the browser scrolls it into view for free — a hand-rolled scroll would
 * fight the one the browser is already doing.
 */

export interface ListKeyboard {
    /** Index of the row the keyboard is on, or -1 before anything is chosen. */
    activeIndex: Ref<number>;
    /** `tabindex` for one row: the active one is reachable, the rest are not. */
    tabIndexFor: (index: number) => number;
    /** Attach to the list container's `@keydown`. */
    onKeydown: (event: KeyboardEvent) => void;
    /** Attach to each row's `@focus`, so clicking and typing agree. */
    onRowFocus: (index: number) => void;
    reset: () => void;
}

export function useListKeyboard(
    items: Ref<any[]>,
    onChoose: (item: any, index: number) => void
): ListKeyboard {
    const activeIndex = ref(-1);

    // A list that shrinks under the cursor — somebody typing in the search box
    // — must not leave the keyboard pointing past the end of it.
    watch(
        () => items.value.length,
        length => {
            if (activeIndex.value >= length) activeIndex.value = length - 1;
        }
    );

    const move = (delta: number) => {
        const length = items.value.length;
        if (length === 0) return;
        const next = activeIndex.value < 0
            ? (delta > 0 ? 0 : length - 1)
            : Math.min(length - 1, Math.max(0, activeIndex.value + delta));
        activeIndex.value = next;
    };

    const onKeydown = (event: KeyboardEvent) => {
        const length = items.value.length;
        switch (event.key) {
            case 'ArrowDown': move(1); break;
            case 'ArrowUp': move(-1); break;
            case 'Home': if (length > 0) activeIndex.value = 0; break;
            case 'End': if (length > 0) activeIndex.value = length - 1; break;
            case 'Enter':
            case ' ': {
                const item = items.value[activeIndex.value];
                if (!item) return;
                onChoose(item, activeIndex.value);
                break;
            }
            default:
                // Everything else belongs to whatever else is listening —
                // typing in the search box, above all.
                return;
        }
        // Only for keys that were handled: swallowing the rest would stop the
        // page scrolling and stop people typing.
        event.preventDefault();
    };

    return {
        activeIndex,
        tabIndexFor: (index: number) =>
            index === activeIndex.value || (activeIndex.value < 0 && index === 0) ? 0 : -1,
        onKeydown,
        onRowFocus: (index: number) => { activeIndex.value = index; },
        reset: () => { activeIndex.value = -1; },
    };
}
