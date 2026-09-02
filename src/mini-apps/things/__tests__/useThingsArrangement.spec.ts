import { describe, it, expect } from 'vitest';
import { useThingsArrangement } from '../composables/useThingsArrangement';

/**
 * What a type leads with when nobody has configured it.
 *
 * Suggested columns are the first thing anyone sees about a kind of thing, and
 * getting them wrong is visible immediately: a brand-new type carries only
 * `title`, `type` and the two timestamps, so suggesting from that list put a
 * raw ISO timestamp under every row and said nothing about what the thing was.
 */
describe('what a list suggests showing', () => {
  it('leads with the fields that are characteristic of the type', () => {
    const arrange = useThingsArrangement();
    arrange.suggestColumns(['author', 'rating', 'status', 'title', 'type', 'updated_at']);
    expect(arrange.columns.value).toEqual(['author', 'rating', 'status']);
  });

  /**
   * True of every node, characteristic of none. Still offered in the menus —
   * `updated_at` is the default sort — just never what a type leads with.
   */
  it('suggests nothing for a type that has no fields of its own', () => {
    const arrange = useThingsArrangement();
    arrange.suggestColumns(['title', 'type', 'created_at', 'updated_at', 'node_id']);
    expect(arrange.columns.value).toEqual([]);
    expect(arrange.sortableFrom(['title', 'created_at', 'updated_at'])).toContain('updated_at');
  });

  it('never offers identity or bookkeeping as something to arrange by', () => {
    const arrange = useThingsArrangement();
    const fields = arrange.arrangeableFrom(['node_id', 'timestamp', 'type', 'author']);
    expect(fields).toEqual(['author']);
  });

  /**
   * The menus win over anything typed by hand, because `parse_query` assigns
   * rather than merges and this goes last. Nothing has to be stripped out of
   * what the user wrote, so nothing they wrote disappears silently.
   */
  it('appends its arrangement after whatever was typed', () => {
    const arrange = useThingsArrangement();
    arrange.sortField.value = 'rating';
    arrange.sortDescending.value = true;
    arrange.columns.value = ['author'];

    const composed = arrange.compose('type:book sort:title');
    expect(composed.startsWith('type:book sort:title')).toBe(true);
    expect(composed.endsWith('sort:-rating columns:author')).toBe(true);
  });

  /**
   * `QueryRow.cells` holds only the columns that were asked for, so grouping by
   * a field nobody selected as a column would group on nothing.
   */
  it('asks for the group key even when it is not a chosen column', () => {
    const arrange = useThingsArrangement();
    arrange.columns.value = ['author'];
    arrange.groupBy.value = 'status';
    expect(arrange.compose('type:book')).toContain('columns:author,status');
  });
});
