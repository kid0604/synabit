import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { nextTick } from 'vue';
import { mount } from '@vue/test-utils';
import GraphView from '../GraphView.vue';

/**
 * The graph draws to a canvas, so what it did is only observable in the calls it
 * made. This context records them, split into frames — every `draw()` starts
 * with `setTransform`, so that call is the frame boundary.
 */
interface Frame {
  transform: number[];
  circles: Array<{ x: number, y: number, r: number }>;
  labels: Array<{ text: string, font: string }>;
}

const last = (frames: Frame[]): Frame | undefined => frames[frames.length - 1];

const recorder = () => {
  const frames: Frame[] = [];
  let font = '';
  const ctx: any = {
    canvas: null as HTMLCanvasElement | null,
    setTransform: (...t: number[]) => frames.push({ transform: t, circles: [], labels: [] }),
    clearRect: () => {}, translate: () => {}, scale: () => {},
    save: () => {}, restore: () => {}, beginPath: () => {},
    moveTo: () => {}, lineTo: () => {}, stroke: () => {}, fill: () => {},
    arc: (x: number, y: number, r: number) => last(frames)?.circles.push({ x, y, r }),
    fillText: (text: string) => last(frames)?.labels.push({ text, font }),
  };
  Object.defineProperty(ctx, 'font', { get: () => font, set: (v) => { font = v; } });
  for (const prop of ['fillStyle', 'strokeStyle', 'lineWidth', 'globalAlpha']) {
    Object.defineProperty(ctx, prop, { get: () => undefined, set: () => {} });
  }
  return { ctx, frames };
};

const graphData = () => {
  const types = ['note', 'task', 'event', 'tag', 'person'];
  const nodes = Array.from({ length: 20 }, (_, i) => ({
    id: `n${i}`, item_type: types[i % types.length], title: `Node ${i}`, tags: [],
  }));
  const links = Array.from({ length: 24 }, (_, i) => ({
    source: `n${i % 20}`, target: `n${(i * 7 + 3) % 20}`,
  })).filter(l => l.source !== l.target);
  return { nodes, links };
};

const wait = (ms: number) => new Promise(r => setTimeout(r, ms));

/** Positions in a frame, keyed so two frames can be compared node by node. */
const positions = (frame: Frame) => frame.circles.map(c => `${c.x.toFixed(4)},${c.y.toFixed(4)}`);

describe('GraphView', () => {
  let rec: ReturnType<typeof recorder>;

  beforeEach(() => {
    rec = recorder();
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(rec.ctx);
    vi.stubGlobal('ResizeObserver', class {
      observe() {} unobserve() {} disconnect() {}
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  const mountGraph = async () => {
    const wrapper = mount(GraphView, { props: { graphData: graphData() } });
    await wait(150);   // the component defers init by 100ms to let layout settle
    return wrapper;
  };

  const range = (wrapper: any, label: string) =>
    wrapper.findAll('input[type="range"]').find((i: any) => i.attributes('aria-label') === label)!;

  it('draws every node once the layout starts', async () => {
    const wrapper = await mountGraph();

    expect(rec.frames.length).toBeGreaterThan(0);
    expect(last(rec.frames)!.circles).toHaveLength(20);

    wrapper.unmount();
  });

  it('sizes the backing store to the device pixel ratio', async () => {
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 2 });
    const wrapper = await mountGraph();

    const canvas = wrapper.find('canvas').element as HTMLCanvasElement;
    // jsdom reports clientWidth 0, so the component falls back to the window size.
    expect(canvas.width).toBe(window.innerWidth * 2);
    expect(canvas.height).toBe(window.innerHeight * 2);
    // Every frame compensates with a matching base transform, so the drawing
    // stays the same size on screen and only gets sharper.
    expect(last(rec.frames)!.transform).toEqual([2, 0, 0, 2, 0, 0]);

    wrapper.unmount();
    Object.defineProperty(window, 'devicePixelRatio', { configurable: true, value: 1 });
  });

  it('redraws a display change without moving anything', async () => {
    const wrapper = await mountGraph();
    const before = last(rec.frames)!;

    await range(wrapper, 'Node size').setValue(2);
    await nextTick();

    const after = last(rec.frames)!;
    expect(after).not.toBe(before);
    expect(positions(after)).toEqual(positions(before));       // layout untouched
    expect(after.circles[0].r).toBeCloseTo(before.circles[0].r * 2);  // but redrawn bigger

    wrapper.unmount();
  });

  it('restarts a filtered-out node from where it was, not from scratch', async () => {
    const wrapper = await mountGraph();
    const before = last(rec.frames)!;
    const beforeByTitle = new Map(before.circles.map((c, i) => [before.circles[i], c]));
    expect(beforeByTitle.size).toBe(20);

    const tagsToggle = wrapper.findAll('input[type="checkbox"]')[3];
    await tagsToggle.setValue(false);
    await nextTick();
    const withoutTags = last(rec.frames)!;
    expect(withoutTags.circles.length).toBeLessThan(before.circles.length);

    await tagsToggle.setValue(true);
    await nextTick();
    const restored = last(rec.frames)!;

    expect(restored.circles).toHaveLength(before.circles.length);
    expect(positions(restored)).toEqual(positions(before));

    wrapper.unmount();
  });

  it('ignores a vault reload that did not change the graph', async () => {
    const wrapper = await mountGraph();
    await wait(50);
    const framesBefore = rec.frames.length;

    // A fresh object with identical contents, which is what a vault reload hands over.
    await wrapper.setProps({ graphData: graphData() });
    await nextTick();

    expect(rec.frames.length).toBe(framesBefore);

    wrapper.unmount();
  });

  it('lays out a graph that did change', async () => {
    const wrapper = await mountGraph();
    const data = graphData();
    data.nodes.push({ id: 'new', item_type: 'note', title: 'New', tags: [] });

    await wrapper.setProps({ graphData: data });
    await nextTick();

    expect(last(rec.frames)!.circles).toHaveLength(21);

    wrapper.unmount();
  });
});
