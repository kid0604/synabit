import { describe, it, expect } from 'vitest';
import { isAppOwned, appOwnedKeys, humanizeKey, isAuthoredElsewhere } from '../fieldRegistry';

/**
 * The direction of the list is the design.
 *
 * A denylist hides the few keys we know are ours and shows everything else; an
 * allowlist would show the few keys we know about and hide everything else.
 * The second one renders an `animal` as a blank card, which is the one outcome
 * this screen exists to prevent.
 */
describe('telling the app’s fields from the person’s', () => {
  it('shows a key nobody has ever declared', () => {
    expect(isAppOwned('animal', 'species')).toBe(false);
    expect(isAppOwned('book', 'translator')).toBe(false);
    expect(isAppOwned('spaceship', 'warp_factor')).toBe(false);
  });

  /**
   * A type this file has never heard of has no app fields of its own to hide.
   *
   * `pinned` is the exception and is tested below: it is machinery on every
   * kind, because the pin is an affordance on every kind.
   */
  it('hides nothing of its own on a type it does not know', () => {
    expect(appOwnedKeys('animal')).toEqual([]);
    expect(isAppOwned('animal', 'colour')).toBe(false);
  });

  /**
   * The reason the list is keyed by type rather than global.
   *
   * `full_width` on a note is the editor's layout toggle. `full_width` on an
   * animal is something a person meant, and a global denylist would take it
   * away from them with no way to find out it was gone.
   */
  it('hides a key on the type that owns it and not on any other', () => {
    expect(isAppOwned('note', 'full_width')).toBe(true);
    expect(isAppOwned('animal', 'full_width')).toBe(false);

    expect(isAppOwned('task', 'order')).toBe(true);
    expect(isAppOwned('note', 'order')).toBe(false);
  });

  /** Identity and timestamps are nobody's business on any type. */
  it('hides bookkeeping everywhere, including on types it does not know', () => {
    for (const type of ['note', 'task', 'animal', 'spaceship']) {
      expect(isAppOwned(type, 'node_id'), type).toBe(true);
      expect(isAppOwned(type, 'created_at'), type).toBe(true);
      expect(isAppOwned(type, 'updated_at'), type).toBe(true);
    }
  });

  /**
   * Facts about the thing stay visible even though an app sets them through a
   * picker. The test is whether a person reading the record wants to see it,
   * not whether they typed it.
   */
  it('keeps the fields that describe the thing rather than the screen', () => {
    expect(isAppOwned('task', 'status')).toBe(false);
    expect(isAppOwned('task', 'priority')).toBe(false);
    expect(isAppOwned('task', 'due_date')).toBe(false);
    expect(isAppOwned('note', 'tags')).toBe(false);
    expect(isAppOwned('person', 'birthday')).toBe(false);
    expect(isAppOwned('person', 'relationship_type')).toBe(false);
  });

  it('reads a key as words without losing the key', () => {
    expect(humanizeKey('due_date')).toBe('Due date');
    expect(humanizeKey('full_width')).toBe('Full width');
    expect(humanizeKey('species')).toBe('Species');
    expect(humanizeKey('')).toBe('');
  });
});

/**
 * Which kinds this screen may make, which is not the same as which it may show.
 *
 * Things writes frontmatter and a Markdown body. A whiteboard is a
 * `.whiteboard.json` of nodes and edges, so writing one from here produces a
 * file the scan indexes as a whiteboard, the Whiteboards app lists, and
 * nothing can draw on — a whiteboard by every sign except having anything in
 * it. A `file` node is metadata about something already on disk, and one made
 * from nothing describes nothing.
 */
describe('kinds that are made somewhere else', () => {
  it('knows the ones whose content is not a body of text', () => {
    expect(isAuthoredElsewhere('whiteboard')).toBe(true);
    expect(isAuthoredElsewhere('file')).toBe(true);
    expect(isAuthoredElsewhere('canvas')).toBe(true);
    expect(isAuthoredElsewhere('finance_month')).toBe(true);
  });

  /**
   * The default has to stay "a kind nobody coded for is one Things can make",
   * or the screen that exists to hold invented kinds stops holding them.
   */
  it('claims nothing about a kind it has never heard of', () => {
    expect(isAuthoredElsewhere('animal')).toBe(false);
    expect(isAuthoredElsewhere('book')).toBe(false);
    expect(isAuthoredElsewhere('spaceship')).toBe(false);
  });

  it('leaves the ordinary kinds alone', () => {
    expect(isAuthoredElsewhere('note')).toBe(false);
    expect(isAuthoredElsewhere('task')).toBe(false);
    expect(isAuthoredElsewhere('person')).toBe(false);
  });
});

/**
 * A key that graduated from one kind to all of them.
 *
 * `pinned` belonged to `note` while Notes was the only screen that pinned
 * anything. Things pins whatever it shows, so the pin is now an affordance on
 * every kind and the key behind it is machinery everywhere.
 */
describe('pinning, once anything can be pinned', () => {
  it('is the app’s key on every kind', () => {
    for (const kind of ['note', 'task', 'animal', 'book', 'spaceship']) {
      expect(isAppOwned(kind, 'pinned'), kind).toBe(true);
    }
  });

  /**
   * The cost, stated. An `animal` whose owner meant something of their own by
   * `pinned` no longer sees it as a field — which is why this list holds one
   * key and is not a convenient place to put things.
   */
  it('does not drag the rest of the note editor’s chrome with it', () => {
    expect(isAppOwned('animal', 'full_width')).toBe(false);
    expect(isAppOwned('animal', 'linked_projects')).toBe(false);
  });
});
