import { describe, it, expect, afterEach } from 'vitest';
import { nextTick } from 'vue';
import {
  iconBodyForNodeType, iconForNodeType, setChosenIcons, chosenIconName,
  iconNamed, ICON_NAMES, SUGGESTED_ICONS,
} from '../nodeTypeIcon';

describe('an icon as drawable shapes', () => {
  it('gives back the paths of a known type', () => {
    const body = iconBodyForNodeType('note');
    expect(body).toMatch(/<path|<polyline|<rect|<circle/);
  });
  it('falls back rather than failing for a type nobody wrote code for', () => {
    expect(iconBodyForNodeType('bookshelf')).toBe(iconBodyForNodeType('cá'));
    expect(iconBodyForNodeType('bookshelf').length).toBeGreaterThan(0);
  });
  it('differs between types', () => {
    expect(iconBodyForNodeType('note')).not.toBe(iconBodyForNodeType('task'));
  });
});

/**
 * A kind drawn with the icon somebody picked for it.
 *
 * The code table knows the dozen kinds this app ships screens for and answers
 * `Box` for every other — which is every kind anybody invents, and Things
 * exists for exactly those. Two invented kinds were two identical squares in
 * every list, table and graph.
 */
describe('an icon a kind was given', () => {
  afterEach(() => setChosenIcons([]));

  it('wins over the built-in table', () => {
    const before = iconForNodeType('note');
    setChosenIcons([['note', 'music']]);

    expect(iconForNodeType('note')).toBe(iconNamed('music'));
    expect(iconForNodeType('note')).not.toBe(before);
  });

  it('gives an invented kind something other than the fallback', () => {
    expect(iconForNodeType('animal')).toBe(iconForNodeType('book'));

    setChosenIcons([['animal', 'dog'], ['book', 'book-open']]);

    expect(iconForNodeType('animal')).toBe(iconNamed('dog'));
    expect(iconForNodeType('book')).toBe(iconNamed('book-open'));
    expect(iconForNodeType('animal')).not.toBe(iconForNodeType('book'));
  });

  /**
   * A schema file outlives the build that wrote it and is plain markdown
   * anybody can edit. A name this version has never heard of has to read as no
   * choice, not as a blank mark where a row's icon should be.
   */
  it('ignores a name this version does not know', () => {
    setChosenIcons([['animal', 'holographic-badger']]);

    expect(chosenIconName('animal')).toBeNull();
    expect(iconForNodeType('animal')).toBe(iconNamed('box'));
  });

  it('forgets a choice that was taken back', () => {
    setChosenIcons([['animal', 'dog']]);
    expect(chosenIconName('animal')).toBe('dog');

    setChosenIcons([]);
    expect(chosenIconName('animal')).toBeNull();
    expect(iconForNodeType('animal')).toBe(iconNamed('box'));
  });

  /**
   * The graph caches rendered shapes under the kind's name, which does not
   * change when its icon does — so without clearing it the graph would go on
   * drawing what the kind used to be.
   */
  it('redraws the graph shapes when the choice changes', async () => {
    const asBox = iconBodyForNodeType('animal');

    setChosenIcons([['animal', 'dog']]);
    await nextTick();

    expect(iconBodyForNodeType('animal'), 'the graph kept the old shapes').not.toBe(asBox);
  });
});

/**
 * The names are a promise. Each one goes into somebody's `Schema/<kind>.md`
 * and has to keep resolving in the version they install next, so the set has
 * to be Lucide's own vocabulary rather than a second one invented here — and
 * every name this app puts in front of somebody has to exist.
 */
describe('the names a kind can be stored against', () => {
  it('offers the whole library, aliases included', () => {
    // Around 1,700 icons under around 1,900 names: the extra names are
    // Lucide's own trail of renames, and keeping them is what lets a schema
    // written a year ago go on drawing the same mark.
    expect(ICON_NAMES.length).toBeGreaterThan(1700);
    expect(new Set(ICON_NAMES.map(n => iconNamed(n))).size).toBeGreaterThan(1500);

    // The module exports every icon a second time with an `Icon` suffix. Left
    // in, the picker shows each one twice — `car` and `car-icon` — and half
    // the grid is a duplicate of the other half.
    expect(ICON_NAMES.filter(n => n.endsWith('-icon'))).toEqual([]);
    // And a third time with a `Lucide` prefix.
    expect(ICON_NAMES.filter(n => n.startsWith('lucide-'))).toEqual([]);
  });

  it('uses the names Lucide publishes, so a hand-edited file can be looked up', () => {
    for (const name of ['file-text', 'building-2', 'shopping-cart', 'graduation-cap']) {
      expect(iconNamed(name), `${name} is not a name this build knows`).not.toBeNull();
    }
    // The JavaScript export name is an implementation detail and must not leak
    // into a file somebody reads.
    expect(iconNamed('FileText')).toBeNull();
  });

  /** Every icon on the picker's first page has to draw something. */
  it('suggests only names that resolve', () => {
    expect(SUGGESTED_ICONS.length).toBeGreaterThan(40);
    for (const name of SUGGESTED_ICONS) {
      expect(iconNamed(name), `the picker offers ${name}, which does not exist`).not.toBeNull();
    }
    expect(new Set(SUGGESTED_ICONS).size, 'the same icon is offered twice').toBe(SUGGESTED_ICONS.length);
  });
});
