import { beforeEach, describe, expect, it, vi } from 'vitest';
import { defineComponent, h, nextTick, ref } from 'vue';
import { mount } from '@vue/test-utils';

// The canvas, as far as an export uses it.
const flow = vi.hoisted(() => ({
  viewport: { x: 0, y: 0, zoom: 1 },
  nodes: [] as any[],
  selected: [] as any[],
}));
vi.mock('@vue-flow/core', async () => {
  const { computed } = await import('vue');
  return {
    useVueFlow: () => ({
      setViewport: (v: any) => { flow.viewport = { ...v }; },
      getViewport: () => ({ ...flow.viewport }),
      getNodes: computed(() => flow.nodes),
      getSelectedNodes: computed(() => flow.selected),
      removeSelectedElements: () => { flow.selected = []; },
      addSelectedNodes: () => {},
      findNode: (id: string) => flow.nodes.find((n) => n.id === id),
    }),
  };
});

// The picture, taken when the test says.
const shots = vi.hoisted(() => ({ calls: [] as { options: any; viewport: any; release: () => void }[] }));
vi.mock('html-to-image', () => {
  const take = (_el: unknown, options: any) =>
    new Promise<string>((resolve) => {
      shots.calls.push({ options, viewport: { ...flow.viewport }, release: () => resolve('data:image/png;base64,AA==') });
    });
  return { toPng: take, toJpeg: take, toSvg: take };
});
vi.mock('../imageAssets', () => ({ assetDataUri: async () => null, rotatedOverhang: () => 0 }));
vi.mock('../../../utils/logger', () => ({ logger: { error: () => {}, warn: () => {}, info: () => {} } }));

import { useClipboardExport } from '../composables/useClipboardExport';
import InkLayer from '../components/InkLayer.vue';

const until = async (ok: () => boolean) => {
  for (let i = 0; i < 200 && !ok(); i++) await new Promise((r) => setTimeout(r, 10));
  expect(ok()).toBe(true);
};

const stroke = { id: 's1', type: 'stroke', selected: true, position: { x: 0, y: 0 }, data: { svgPath: 'M0 0L10 10', width: 10, height: 10 } };

function setup() {
  const el = document.createElement('div');
  document.body.appendChild(el);
  const store = {
    currentBoardData: ref({
      title: 'B',
      nodes: [
        { id: 'a', type: 'sticky', position: { x: 0, y: 0 }, data: {} },
        { id: 'c', type: 'comment', position: { x: 5000, y: 5000 }, data: {} },
        stroke,
      ],
      edges: [],
    }),
    backgroundColor: ref('#ffffff'),
  };
  const reselect = vi.fn();
  let api!: ReturnType<typeof useClipboardExport>;
  const Host = defineComponent({
    setup() {
      api = useClipboardExport(store, ref('/vault'), () => el, undefined, reselect);
      return () => h(InkLayer, {
        strokes: [stroke],
        viewport: { x: 0, y: 0, zoom: 1 },
        size: { width: 800, height: 600 },
        everything: api.isExporting.value,
        only: api.exportOnly.value,
        interactive: true,
        marquee: null,
        selectionBox: { x: 0, y: 0, width: 10, height: 10 },
      });
    },
  });
  const wrapper = mount(Host);
  return { api, reselect, wrapper };
}

beforeEach(() => {
  flow.viewport = { x: 7, y: 8, zoom: 1.5 };
  flow.nodes = [
    { id: 'a', type: 'sticky', position: { x: 0, y: 0 }, dimensions: { width: 100, height: 100 } },
    // A comment far away: not in the picture, and not in its bounds.
    { id: 'c', type: 'comment', position: { x: 5000, y: 5000 }, dimensions: { width: 100, height: 100 } },
    // What a folded branch hides: never measured, never waited for.
    { id: 'h', type: 'mindmap', hidden: true, position: { x: -9000, y: -9000 } },
  ];
  flow.selected = [flow.nodes[0]];
  shots.calls = [];
});

describe('exporting', () => {
  it('runs a second export after the first, and puts the view back as it was', async () => {
    const { api } = setup();
    const first = api.exportBoard({ format: 'png', keep: true });
    const second = api.exportBoard({ format: 'png', only: ['a'], keep: true });
    await until(() => shots.calls.length === 1);
    // The second waits: nothing of it has touched the canvas yet.
    await new Promise((r) => setTimeout(r, 50));
    expect(shots.calls).toHaveLength(1);
    expect(api.isExporting.value).toBe(true);
    shots.calls[0].release();
    expect(await first).toBe('data:image/png;base64,AA==');
    await until(() => shots.calls.length === 2);
    shots.calls[1].release();
    expect(await second).toBe('data:image/png;base64,AA==');
    expect(flow.viewport).toEqual({ x: 7, y: 8, zoom: 1.5 });
    expect(api.isExporting.value).toBe(false);
    expect(api.exportOnly.value).toBeNull();
  });

  it('measures the picture without comments or what a folded branch hides', async () => {
    const { api } = setup();
    const started = Date.now();
    const done = api.exportBoard({ format: 'png', keep: true });
    await until(() => shots.calls.length === 1);
    // The sticky and the stroke, with 50 around them.
    expect(shots.calls[0].options.width).toBe(200);
    expect(shots.calls[0].options.height).toBe(200);
    expect(Date.now() - started).toBeLessThan(450);
    shots.calls[0].release();
    await done;
  });

  it('leaves the ink selection box out of the picture, and selects the ink again after', async () => {
    const { api, reselect, wrapper } = setup();
    expect(wrapper.find('.wb-ink__selection').exists()).toBe(true);
    const done = api.exportBoard({ format: 'png', keep: true });
    await until(() => shots.calls.length === 1);
    await nextTick();
    expect(wrapper.find('.wb-ink__selection').exists()).toBe(false);
    shots.calls[0].release();
    await done;
    expect(reselect).toHaveBeenCalledWith(['a', 's1']);
    await nextTick();
    expect(wrapper.find('.wb-ink__selection').exists()).toBe(true);
  });
});
