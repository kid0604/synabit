import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';

import NoteGraph from '../NoteGraph.vue';

// The graph watches its container so a resized sidebar re-lays-out the force
// simulation. jsdom has no such observer and no layout to report; the graph
// already falls back to a fixed size when the element measures zero, so a stub
// that never fires is the honest stand-in.
class NoOpResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', NoOpResizeObserver);

/**
 * What the graph draws for each node.
 *
 * Notes shows one kind of thing, so a glyph on every mark would say the same
 * word forty times — discs stay its default and this pins that down. Things
 * draws whatever the vault holds, and a graph around a `book` reaching people,
 * tasks and notes is exactly where the shape earns its place.
 */

const props = {
  currentNoteId: 'Book/dune.md',
  currentNoteTitle: 'Dune',
  currentNodeType: 'book',
  tags: ['scifi'],
  outgoingLinks: [] as string[],
  backlinks: [
    { id: 'People/frank.md', title: 'Frank Herbert', nodeType: 'person' },
    { id: 'Tasks/reread.md', title: 'Reread it', nodeType: 'task' },
  ],
  allNotes: [] as Array<{ id: string; title: string; nodeType?: string }>,
};

const draw = (marks?: 'dots' | 'icons') =>
  mount(NoteGraph, { props: { ...props, ...(marks ? { marks } : {}) }, attachTo: document.body });

describe('the marks in the graph', () => {
  it('draws plain discs by default, which is what Notes gets', () => {
    const g = draw();
    const svg = g.element.querySelector('svg')!;

    expect(svg.querySelectorAll('circle').length, 'a disc per node').toBeGreaterThan(0);
    expect(
      svg.querySelectorAll('path[d]').length === 0 || svg.querySelectorAll('g[transform*="scale"]').length,
      'no glyphs unless asked for',
    ).toBeFalsy();

    g.unmount();
  });

  it('puts a glyph on each node when asked, and keeps the disc under it', () => {
    const g = draw('icons');
    const svg = g.element.querySelector('svg')!;

    // The disc carries the colour the legend explains; dropping it would trade
    // one fact for another.
    expect(svg.querySelectorAll('circle').length, 'the discs are gone').toBeGreaterThan(0);

    const glyphs = svg.querySelectorAll('g[transform*="scale"]');
    // Centre, two backlinks — and not the tag.
    expect(glyphs.length, 'one glyph per node that has a kind').toBe(3);
    for (const glyph of glyphs) {
      expect(glyph.innerHTML, 'a glyph with no shapes in it').toMatch(/<path|<polyline|<rect|<circle/);
    }

    g.unmount();
  });

  /** A tag is not a node and has no kind, so no icon would be true. */
  it('leaves a tag as a disc even in icon mode', () => {
    const g = draw('icons');
    const svg = g.element.querySelector('svg')!;

    // Not every `<circle>` is a node. Lucide's `person` glyph is built from
    // one, so counting them all reports five nodes for four — which looks
    // exactly like a tag being drawn twice.
    const glyphs = svg.querySelectorAll('g[transform*="scale"]');
    const marks = [...svg.querySelectorAll('circle')].filter(
      c => !c.closest('g[transform*="scale"]'),
    );

    expect(marks.length, 'four nodes: the book, two backlinks, one tag').toBe(4);
    expect(glyphs.length, 'the tag was given an icon it has no claim to').toBe(3);

    g.unmount();
  });

  /**
   * The redraw is guarded by a fingerprint of the graph's contents. Left out
   * of it, flipping the switch changes nothing until something else moves —
   * which reads as the switch being broken rather than as caching.
   *
   * Flipped twice on purpose. `lastGraphFingerprint` starts empty and the
   * mount draws without setting it, so the *first* flip gets through whatever
   * the fingerprint contains — a single flip proves nothing here, and passed
   * happily with the fix removed.
   */
  it('redraws every time the switch is flipped, not only the first time', async () => {
    const glyphs = () => g.element.querySelectorAll('g[transform*="scale"]').length;
    const settle = () => new Promise(r => setTimeout(r, 350));

    const g = draw('dots');
    expect(glyphs()).toBe(0);

    await g.setProps({ marks: 'icons' });
    await settle();
    expect(glyphs(), 'the switch did not reach the drawing').toBe(3);

    await g.setProps({ marks: 'dots' });
    await settle();
    expect(glyphs(), 'the switch works once and then sticks').toBe(0);

    g.unmount();
  });
});
