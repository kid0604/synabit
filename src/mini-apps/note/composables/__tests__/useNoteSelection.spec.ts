import { describe, it, expect } from 'vitest';
import { useNoteSelection } from '../useNoteSelection';

/** One page of the manager, in the order the reader sees it. */
const page = ['a', 'b', 'c', 'd', 'e'];

describe('useNoteSelection', () => {
  it('ticks and unticks one row', () => {
    const s = useNoteSelection();

    s.toggle('b', page);
    expect(s.ids.value).toEqual(['b']);
    expect(s.active.value).toBe(true);

    s.toggle('b', page);
    expect(s.ids.value).toEqual([]);
    expect(s.active.value).toBe(false);
  });

  it('extends from the last row ticked on its own', () => {
    const s = useNoteSelection();
    s.toggle('b', page);
    s.toggle('d', page, true);

    expect(s.ids.value.sort()).toEqual(['b', 'c', 'd']);
  });

  it('extends upwards as readily as down', () => {
    const s = useNoteSelection();
    s.toggle('d', page);
    s.toggle('b', page, true);

    expect(s.ids.value.sort()).toEqual(['b', 'c', 'd']);
  });

  /**
   * The anchor staying put is what makes a second shift-click a correction
   * rather than an extension of whatever the first one happened to reach.
   */
  it('re-measures a second range from the same anchor', () => {
    const s = useNoteSelection();
    s.toggle('a', page);
    s.toggle('e', page, true);
    expect(s.ids.value).toHaveLength(5);

    s.toggle('b', page, true);
    // Nothing is removed — the earlier range is still ticked — but the new
    // range was measured from `a`, not from `e`.
    expect(s.ids.value.sort()).toEqual(['a', 'b', 'c', 'd', 'e']);
  });

  it('falls back to a plain tick when the anchor has scrolled out of reach', () => {
    // A page turned between the two clicks. A range from a row nobody can see
    // would select something the reader did not point at.
    const s = useNoteSelection();
    s.toggle('b', page);

    const nextPage = ['f', 'g', 'h'];
    s.toggle('h', nextPage, true);

    expect(s.ids.value.sort()).toEqual(['b', 'h']);
  });

  it('takes the whole page, then gives it back', () => {
    const s = useNoteSelection();

    s.toggleAll(page);
    expect(s.ids.value.sort()).toEqual([...page].sort());
    expect(s.allVisibleSelected(page)).toBe(true);

    s.toggleAll(page);
    expect(s.ids.value).toEqual([]);
  });

  /**
   * "Select all" means this page. A selection carried over from another one is
   * still real and must survive.
   */
  it('leaves rows from another page alone when taking this one', () => {
    const s = useNoteSelection();
    s.toggle('z', ['z']);
    s.toggleAll(page);

    expect(s.ids.value).toContain('z');
    expect(s.count.value).toBe(6);

    s.toggleAll(page);
    expect(s.ids.value).toEqual(['z']);
  });

  it('reads as partly ticked only when some of the visible rows are', () => {
    const s = useNoteSelection();
    expect(s.someVisibleSelected(page)).toBe(false);

    s.toggle('b', page);
    expect(s.someVisibleSelected(page)).toBe(true);
    expect(s.allVisibleSelected(page)).toBe(false);

    s.toggleAll(page);
    expect(s.someVisibleSelected(page)).toBe(false);
    expect(s.allVisibleSelected(page)).toBe(true);
  });

  it('an empty page is neither all nor partly ticked', () => {
    const s = useNoteSelection();
    expect(s.allVisibleSelected([])).toBe(false);
    expect(s.someVisibleSelected([])).toBe(false);
  });

  it('clearing forgets the anchor too', () => {
    const s = useNoteSelection();
    s.toggle('b', page);
    s.clear();

    // With the anchor gone, a shift-click is an ordinary tick again rather
    // than a range reaching back to a row nobody has selected.
    s.toggle('d', page, true);
    expect(s.ids.value).toEqual(['d']);
  });
});
