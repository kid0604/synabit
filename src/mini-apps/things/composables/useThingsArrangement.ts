import { ref, computed } from 'vue';
import { isAppOwned } from '../../../shared/fieldRegistry';

/**
 * Keys the app writes, which are never offered as something to arrange by.
 *
 * `node_id` and `timestamp` are identity and bookkeeping — sorting a list by
 * a UUID is a sort by nothing. `type` is excluded because the list is almost
 * always already filtered to one type, so it would group everything into a
 * single heap.
 */
const NOT_ARRANGEABLE = new Set(['node_id', 'timestamp', 'type']);

/**
 * Fields that are true of every node and characteristic of none.
 *
 * Kept out of the *suggestions* only — they are still offered in the menus,
 * and `updated_at` is the default sort. What they must not be is what a type
 * leads with: a type nobody has put a field on yet has only these, so
 * suggesting them fills the list with raw ISO timestamps and says nothing
 * about what the thing is.
 */
const NOT_CHARACTERISTIC = new Set(['title', 'created_at', 'updated_at']);

/**
 * Columns the engine can sort on without reading frontmatter.
 *
 * `SORTABLE_COLUMNS` in `search.rs`, minus `path`, which is the id and reads
 * as noise in a menu. Offered on every type, because every node has them.
 */
const BUILT_IN_SORTS = ['title', 'updated_at', 'created_at'];

/**
 * How a list is arranged, and how that reaches the engine.
 *
 * Sorting and columns are pushed down as query tokens — `sort:-rating`,
 * `columns:title,author` — because the engine has read frontmatter as data
 * since long before this screen existed, and doing it in the browser would mean
 * sorting one page of a result and calling it a sorted list.
 *
 * Grouping is done here, on the rows that came back. There is no `GROUP BY` in
 * the query language, and adding one would change what a page means: a group
 * cut off by `limit` is a group with a wrong count in its header.
 */
export function useThingsArrangement() {
  const sortField = ref<string>('updated_at');
  const sortDescending = ref(true);
  const groupBy = ref<string>('');
  const columns = ref<string[]>([]);

  /**
   * Appended to whatever the user typed, and appended *last* on purpose.
   *
   * `parse_query` assigns rather than merges — a later `sort:` overwrites an
   * earlier one — so a hand-typed `sort:` in the box is overridden by the menu
   * instead of colliding with it. Nothing has to be stripped out of what the
   * user wrote, which means nothing they wrote disappears silently either.
   */
  const compose = (typed: string): string => {
    const parts = [typed.trim()];

    if (sortField.value) {
      parts.push(`sort:${sortDescending.value ? '-' : ''}${sortField.value}`);
    }

    // The group key has to come back with the rows or there is nothing to
    // group on: `QueryRow.cells` holds only the columns that were asked for.
    const requested = [...columns.value];
    if (groupBy.value && !requested.includes(groupBy.value)) {
      requested.push(groupBy.value);
    }
    if (requested.length) {
      parts.push(`columns:${requested.join(',')}`);
    }

    return parts.filter(Boolean).join(' ');
  };

  /**
   * What this type's nodes actually carry, as things to arrange by.
   *
   * Read from the vault rather than from a list in the code — which is the
   * whole point. Write `energy: low` into one file and `energy` is in the sort
   * menu the next time it is opened, without anybody having been told.
   */
  const arrangeableFrom = (observedFields: string[]): string[] =>
    observedFields.filter(f => !NOT_ARRANGEABLE.has(f));

  const sortableFrom = (observedFields: string[]): string[] => {
    const fromVault = arrangeableFrom(observedFields).filter(f => !BUILT_IN_SORTS.includes(f));
    return [...BUILT_IN_SORTS, ...fromVault];
  };

  const toggleColumn = (field: string) => {
    const at = columns.value.indexOf(field);
    if (at >= 0) columns.value.splice(at, 1);
    else columns.value.push(field);
  };

  /**
   * A sensible starting set for a type nobody configured.
   *
   * Three fields, in the order the vault reports them — which is by how many
   * nodes carry each, so the most characteristic fields come first. This is the
   * moment the app looks like it already knows about books.
   */
  /**
   * Columns worth looking at, for a kind nobody has arranged by hand.
   *
   * The kind matters, because what counts as noise is scoped to it: `pinned`
   * and `full_width` are on every note and belong to the note editor, and a
   * table of them is a hundred rows reading `false · false`. `fieldRegistry`
   * already knows which keys are the app's on which kind, and it was simply
   * not being asked.
   */
  const suggestColumns = (observedFields: string[], nodeType = '') => {
    columns.value = arrangeableFrom(observedFields)
      .filter(f => !NOT_CHARACTERISTIC.has(f) && !isAppOwned(nodeType, f))
      .slice(0, 3);
  };

  const reset = () => {
    sortField.value = 'updated_at';
    sortDescending.value = true;
    groupBy.value = '';
    columns.value = [];
  };

  const isArranged = computed(
    () => sortField.value !== 'updated_at' || !sortDescending.value
      || !!groupBy.value || columns.value.length > 0,
  );

  return {
    sortField, sortDescending, groupBy, columns, isArranged,
    compose, arrangeableFrom, sortableFrom, toggleColumn, suggestColumns, reset,
  };
}
