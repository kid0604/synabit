import { describe, it, expect, vi, afterEach } from 'vitest';
import { nextTick } from 'vue';
import { mount } from '@vue/test-utils';
import GraphView from '../GraphView.vue';
import type { TimeFrame } from '../../timeFrame';

/**
 * What looking back costs on a 2,000-node vault, measured rather than guessed.
 *
 * The gate for the time strip, §10 of `docs/timeline-2026-09-17.md` is that dragging the
 * strip stays smooth on a vault of 2,000 nodes. Moving the strip never re-runs
 * the layout, so each step is one redraw: decide what is in the picture, then
 * issue every draw call for it. That is measured here, one step per month
 * across eighteen years.
 *
 * jsdom cannot paint, so the canvas is a stub that accepts every call and
 * does nothing. Painting is real work in a WebView and is not claimed about.
 *
 * ```bash
 * npx vitest run scrubCost --reporter=verbose
 * ```
 */

const NODES = 2000;
const LINKS = 4000;
const MONTHS = 216;

/** Deterministic, so two runs measure the same vault. */
const random = (() => {
  let seed = 42;
  return () => {
    seed = (seed * 1664525 + 1013904223) % 4294967296;
    return seed / 4294967296;
  };
})();

const monthDay = (i: number) => {
  const year = 2009 + Math.floor(i / 12);
  const month = String((i % 12) + 1).padStart(2, '0');
  return `${year}-${month}-28`;
};

const aVault = () => {
  const types = ['note', 'task', 'event', 'person', 'file'];
  const nodes = Array.from({ length: NODES }, (_, i) => ({
    id: `n${i}`, item_type: types[i % types.length], title: `Node ${i}`, tags: [],
  }));
  const links = Array.from({ length: LINKS }, () => ({
    source: `n${Math.floor(random() * NODES)}`,
    target: `n${Math.floor(random() * NODES)}`,
  })).filter(l => l.source !== l.target);

  const first_seen: Record<string, string> = {};
  for (const node of nodes) first_seen[node.id] = monthDay(Math.floor(random() * MONTHS));
  const died_on: Record<string, string> = {};
  for (let i = 3; i < 250; i += 5) died_on[`n${i}`] = monthDay(Math.floor(random() * MONTHS));
  const timed = links.slice(0, 200).map(l => {
    const start = Math.floor(random() * MONTHS);
    return { ...l, since: monthDay(start), until: monthDay(Math.min(MONTHS - 1, start + 24)), met: 0 };
  });

  const frame: TimeFrame = { first_seen, died_on, links: timed, density: [], earliest: '2009-01' };
  return { graph: { nodes, links }, frame };
};

const stubCanvas = () => new Proxy({} as Record<string, unknown>, {
  get: (target, prop) => (prop in target ? target[prop as string] : () => {}),
  set: (target, prop, value) => { target[prop as string] = value; return true; },
});

const wait = (ms: number) => new Promise(r => setTimeout(r, ms));

describe('looking back on a large vault', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('redraws a month of a 2,000-node graph inside a frame', async () => {
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(stubCanvas() as any);
    vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} });
    vi.stubGlobal('Path2D', class { arc() {} rect() {} roundRect() {} moveTo() {} lineTo() {} });

    const { graph, frame } = aVault();
    const wrapper = mount(GraphView, { props: { graphData: graph, timeFrame: frame, atDate: monthDay(0) } });
    await wait(150);

    const samples: number[] = [];
    for (let i = 0; i < MONTHS; i++) {
      const started = performance.now();
      await wrapper.setProps({ atDate: monthDay(i) });
      await nextTick();
      samples.push(performance.now() - started);
    }
    wrapper.unmount();

    const sorted = [...samples].sort((a, b) => a - b);
    const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
    console.table({
      nodes: NODES,
      links: graph.links.length,
      steps: samples.length,
      'median ms': at(0.5).toFixed(2),
      'p95 ms': at(0.95).toFixed(2),
      'max ms': sorted[sorted.length - 1].toFixed(2),
    });

    expect(samples).toHaveLength(MONTHS);
    // Sixteen milliseconds is a frame at 60 Hz. Script alone must fit inside
    // it with room to spare, or painting has nothing left to work with.
    expect(at(0.5)).toBeLessThan(16);
  });
});
