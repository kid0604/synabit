import { computed, ref } from 'vue';

/**
 * Ticking off several notes at once, so one action can reach all of them.
 *
 * The Note Manager is the only list that shows every note rather than the
 * recent handful, so it is the only place worth selecting in — and the reason
 * this exists at all is the job it makes possible: a sync that left thirteen
 * copies of the same day is thirteen deletes, and thirteen trips through a
 * context menu is not a feature, it is a chore with a mouse attached.
 *
 * ## What it deliberately does not know
 *
 * Which notes are on screen. Every call that needs that is handed the visible
 * ids, because "all" and "the range between here and there" mean the rows the
 * reader can see — the current page, under the current filter — and not the
 * whole vault. Keeping that out of here is what lets the manager change how it
 * pages without this file caring.
 */
export function useNoteSelection() {
  /**
   * A `Set` in a `ref`, which Vue tracks by membership: a template asking
   * `has(id)` re-renders when that one id is added or removed, and not when
   * some other row is ticked.
   */
  const selected = ref(new Set<string>());

  /**
   * Where a shift-click measures from.
   *
   * The last row ticked on its own, not the last row touched — so extending a
   * range twice from the same anchor replaces the range rather than growing it
   * from wherever the previous one happened to end.
   */
  const anchor = ref<string | null>(null);

  const isSelected = (id: string) => selected.value.has(id);
  const count = computed(() => selected.value.size);
  const ids = computed(() => [...selected.value]);
  const active = computed(() => selected.value.size > 0);

  const clear = () => {
    selected.value = new Set();
    anchor.value = null;
  };

  /** Tick one row, or with `extend`, everything between it and the anchor. */
  const toggle = (id: string, visibleIds: string[], extend = false) => {
    const from = anchor.value === null ? -1 : visibleIds.indexOf(anchor.value);
    const to = visibleIds.indexOf(id);

    // An anchor on a row that is no longer visible — a page turned, a filter
    // narrowed — cannot describe a range any reader would recognise, so this
    // falls back to an ordinary tick rather than selecting something arbitrary.
    if (!extend || from === -1 || to === -1) {
      const next = new Set(selected.value);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      selected.value = next;
      anchor.value = id;
      return;
    }

    const next = new Set(selected.value);
    const [lo, hi] = from <= to ? [from, to] : [to, from];
    for (let i = lo; i <= hi; i++) next.add(visibleIds[i]);
    selected.value = next;
    // The anchor stays put, so a second shift-click re-measures from the same
    // row instead of from wherever the last one ended.
  };

  /** All of the visible rows, or none of them if they are already all ticked. */
  const toggleAll = (visibleIds: string[]) => {
    const allTicked = visibleIds.length > 0 && visibleIds.every((id) => selected.value.has(id));
    const next = new Set(selected.value);
    for (const id of visibleIds) {
      if (allTicked) next.delete(id);
      else next.add(id);
    }
    selected.value = next;
    anchor.value = allTicked ? null : (visibleIds[visibleIds.length - 1] ?? null);
  };

  /**
   * Whether the header tick should read as all, none, or some.
   *
   * Only the visible rows count. A selection carried over from another page is
   * still real, but a header box claiming "all" while showing a page of
   * unticked rows would be describing something the reader cannot see.
   */
  const allVisibleSelected = (visibleIds: string[]) =>
    visibleIds.length > 0 && visibleIds.every((id) => selected.value.has(id));

  const someVisibleSelected = (visibleIds: string[]) =>
    visibleIds.some((id) => selected.value.has(id)) && !allVisibleSelected(visibleIds);

  return {
    selected,
    ids,
    count,
    active,
    isSelected,
    toggle,
    toggleAll,
    clear,
    allVisibleSelected,
    someVisibleSelected,
  };
}
