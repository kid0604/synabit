/**
 * The kit a Syn visual runs with (`src-tauri/src/syn/visual/kit.js`), run
 * here as a page would run it: standalone, with no host to answer.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

declare const __dirname: string;

const KIT = readFileSync(resolve(__dirname, '../../../../src-tauri/src/syn/visual/kit.js'), 'utf8');

interface Kit {
  number(value: number, digits?: number): string;
  expect(name: string, actual: unknown, expected: unknown, tolerance?: number): boolean;
  chart(el: Element, opts: unknown): { x: (v: number) => number; y: ((v: number) => number)[] };
  controls(el: Element, specs: unknown[], onChange?: (v: Record<string, unknown>) => void): { values(): Record<string, unknown> };
  stats(el: Element, items: unknown[]): void;
  layout(el: Element, kind: string): Record<string, HTMLElement>;
  solve(f: (x: number) => number, lo: number, hi: number): number;
  peak(f: (x: number) => number, lo: number, hi: number, which?: 'min'): { x: number; y: number };
  integrate(f: (x: number) => number, a: number, b: number, n?: number): number;
  ode(
    rates: (s: Record<string, number>, t: number) => Record<string, number>,
    state: Record<string, number>,
    t0: number,
    t1: number,
    opts?: { dt?: number; method?: 'verlet'; pairs?: Record<string, string> },
  ): (Record<string, number> & { t: number })[];
  sim(el: Element, opts: unknown): { reset(s?: unknown): void; state(): Record<string, number>; time(): number };
  stepper(el: Element, opts: unknown): { go(i: number): void; at(): number };
  graph(el: Element, opts: unknown): { svg: SVGSVGElement };
}

let kit: Kit;
beforeAll(() => {
  document.documentElement.lang = 'vi-VN';
  new Function(KIT)();
  kit = (window as unknown as { synabit: Kit }).synabit;
});

describe("the kit's numbers", () => {
  it('writes them the way the locale does, rounded', () => {
    expect(kit.number(1500000)).toBe('1.500.000');
    expect(kit.number(0.1 + 0.2)).toBe('0,3');
  });

  /** An energy drift of -0.00001%, seen 2026-10-27 as "-0%". */
  it('never shows a minus on a number that rounds to zero', () => {
    expect(kit.number(-0.00001, 4)).toBe('0');
    expect(kit.number(-0)).toBe('0');
    expect(kit.number(-0.004)).toBe('0');
    expect(kit.number(-0.006)).toBe('-0,01');
  });
});

/**
 * What a visual claims of itself (plan 2026-10-28, §2.2). `synabit.lint` reads
 * these back in a page; `e2e/visual-lint/` checks it there, drawn in WebKit.
 */
describe("the kit's claims", () => {
  /** The tax of 2026-10-28: 5P = 120 + 3t, derived without the 3. */
  it('checks a derived number substituted back', () => {
    const t = 2;
    const wrong = (t + 120) / 5;
    expect(kit.expect('demand meets supply', 100 - 2 * wrong, 3 * (wrong - t) - 20, 1e-6)).toBe(false);
    const right = (120 + 3 * t) / 5;
    expect(kit.expect('demand meets supply', 100 - 2 * right, 3 * (right - t) - 20, 1e-6)).toBe(true);
  });

  it('holds float noise within a part in a billion without a tolerance, and nothing past it', () => {
    expect(kit.expect('sum', 0.1 + 0.2, 0.3)).toBe(true);
    expect(kit.expect('sum', 0.3001, 0.3)).toBe(false);
    expect(kit.expect('nothing', Number.NaN, 1, 10)).toBe(false);
    expect(kit.expect('a label', 'A', 'A')).toBe(true);
  });
});

/** The parts a visual is built from (plan 2026-10-28, §2.4, PR Q5). */
describe("the kit's components", () => {
  const fresh = () => {
    const el = document.createElement('div');
    document.body.appendChild(el);
    return el;
  };
  const texts = (el: Element) => [...el.querySelectorAll('text')].map((t) => t.textContent);

  it('draws one panel per measure, each with its own axis and unit, on one x axis', () => {
    const el = fresh();
    kit.chart(el, {
      x: { label: 'năm', min: 0, max: 20 },
      panels: [
        { y: { label: 'dư nợ', unit: 'triệu ₫' }, series: [{ name: 'dư nợ', fn: (t: number) => 500 - 25 * t }] },
        { y: { label: 'trả mỗi tháng', unit: 'triệu ₫' }, series: [{ name: 'trả', fn: () => 4.5 }] },
      ],
    });
    const svgs = el.querySelectorAll('svg[data-synabit="chart"]');
    expect(svgs.length).toBe(2);
    expect(texts(svgs[0])).toContain('dư nợ (triệu ₫)');
    expect(texts(svgs[1])).toContain('trả mỗi tháng (triệu ₫)');
    // The x title once, under the last panel.
    expect(texts(el).filter((t) => t === 'năm').length).toBe(1);
    expect(texts(svgs[1])).toContain('năm');
    // Round ticks, written as the locale writes them.
    expect(texts(svgs[0])).toEqual(expect.arrayContaining(['0', '100', '500']));
    // A function is drawn through many points; two series make a legend.
    expect(svgs[0].querySelector('polyline')!.getAttribute('points')!.split(' ').length).toBeGreaterThan(100);
    expect(el.textContent).toContain('trả');
  });

  it('shades an area under a line with its amount, and marks a point', () => {
    const el = fresh();
    kit.chart(el, {
      x: { label: 'thời gian', unit: 's', min: 0, max: 10 },
      y: { label: 'vận tốc', unit: 'm/s' },
      series: [{ name: 'v', fn: (t: number) => Math.min(10, 2.5 * t) }],
      areas: [{ name: 'tăng tốc', below: 'v', from: 0, to: 4, label: '20 m' }],
      marks: [{ x: 4, y: 10, label: 'đạt 10 m/s' }],
    });
    expect(el.querySelectorAll('polygon').length).toBe(1);
    expect(texts(el)).toEqual(expect.arrayContaining(['20 m', 'đạt 10 m/s']));
    // A label sits on a box, so a line under it does not cross its letters.
    const label = [...el.querySelectorAll('text')].find((t) => t.textContent === '20 m')!;
    expect(label.previousElementSibling?.tagName).toBe('rect');
    expect(el.textContent).toContain('tăng tốc');
  });

  it('draws bars over categories, and drawn again draws only the new one', () => {
    const el = fresh();
    const opts = { x: { categories: ['Gốc', 'Lãi'] }, y: { label: 'tổng', unit: 'triệu ₫' }, series: [{ name: 'a', values: [500, 580] }, { name: 'b', values: [500, 452] }] };
    kit.chart(el, opts);
    kit.chart(el, opts);
    expect(el.querySelectorAll('svg').length).toBe(1);
    expect(el.querySelectorAll('rect').length).toBe(4);
    expect(texts(el)).toEqual(expect.arrayContaining(['Gốc', 'Lãi']));
  });

  it('gives each control its label, value, unit and range, and says every change', () => {
    const el = fresh();
    const seen: Record<string, unknown>[] = [];
    kit.controls(
      el,
      [
        { id: 'h', label: 'Độ cao', min: 5, max: 50, step: 5, value: 20, unit: 'm', help: 'Thả từ độ cao này', presets: [{ label: 'Cao', value: 50 }] },
        { id: 'where', label: 'Nơi thả', options: [{ value: 'earth', label: 'Trái Đất' }, { value: 'moon', label: 'Mặt Trăng' }], value: 'moon' },
        { id: 'air', label: 'Có lực cản', type: 'checkbox', value: false },
      ],
      (v) => seen.push(v),
    );
    expect(seen[0]).toEqual({ h: 20, where: 'moon', air: false });
    expect(el.textContent).toContain('20 m');
    expect(el.textContent).toContain('5 m');
    expect(el.textContent).toContain('50 m');
    expect(el.textContent).toContain('Thả từ độ cao này');
    const slider = el.querySelector('input[type="range"]') as HTMLInputElement;
    expect(el.querySelector(`label[for="${slider.id}"]`)?.textContent).toBe('Độ cao');
    slider.value = '35';
    slider.dispatchEvent(new Event('input'));
    expect(seen.at(-1)).toMatchObject({ h: 35 });
    expect(el.textContent).toContain('35 m');
    (el.querySelector('button') as HTMLButtonElement).click();
    expect(seen.at(-1)).toMatchObject({ h: 50 });
  });

  it('writes stats in the locale, with their unit and note', () => {
    const el = fresh();
    kit.stats(el, [{ label: 'Tổng lãi', value: 579700000, unit: '₫', digits: 0, note: 'niên kim' }, { label: 'Kết quả', value: 'đúng' }]);
    expect(el.textContent).toContain('579.700.000');
    expect(el.textContent).toContain('₫');
    expect(el.textContent).toContain('niên kim');
    expect(el.textContent).toContain('đúng');
  });

  it('frames a visual in named slots', () => {
    expect(Object.keys(kit.layout(fresh(), 'side')).sort()).toEqual(['controls', 'stage', 'stats']);
    expect(Object.keys(kit.layout(fresh(), 'stack')).sort()).toEqual(['controls', 'stage', 'stats']);
    expect(Object.keys(kit.layout(fresh(), 'compare')).sort()).toEqual(['a', 'aStats', 'b', 'bStats', 'controls']);
  });
});

/**
 * Numbers worked out from the model's own functions (plan 2026-10-28, §2.2,
 * PR Q7): the tax of 2026-10-28 was drawn from a formula derived by hand, and
 * when its own check failed the model could not find a better one.
 */
describe("the kit's numbers from the model", () => {
  it('solves an equilibrium from demand and supply, with no formula derived', () => {
    const t = 2;
    const demand = (p: number) => 100 - 2 * p;
    const supply = (p: number) => 3 * (p - t) - 20; // the seller keeps p − t
    const p = kit.solve((x) => demand(x) - supply(x), 0, 100);
    expect(p).toBeCloseTo(25.2, 10);
  });

  it('looks inside the range for a crossing, and says when there is none', () => {
    expect(Math.abs(kit.solve((x) => x * x - 1, -2, 2))).toBeCloseTo(1, 10);
    expect(() => kit.solve((x) => x * x + 1, -2, 2)).toThrow(/does not cross zero/);
  });

  it('finds a peak over the whole range: the Gibbs overshoot of five terms', () => {
    const partial = (x: number) => (4 / Math.PI) * [1, 3, 5, 7, 9].reduce((s, k) => s + Math.sin(k * x) / k, 0);
    const top = kit.peak(partial, 0.01, Math.PI - 0.01);
    expect(top.y).toBeCloseTo(1.1824, 3);
    expect(kit.peak((x) => (x - 2) ** 2, 0, 5, 'min').x).toBeCloseTo(2, 6);
    // A broad hill and a narrow, higher one: a search from one place finds the hill.
    const twoPeaks = (x: number) => Math.exp(-((x - 1) ** 2)) + 2 * Math.exp(-(((x - 4) / 0.05) ** 2));
    expect(kit.peak(twoPeaks, 0, 5).x).toBeCloseTo(4, 4);
  });

  it('integrates by Simpson’s rule', () => {
    expect(kit.integrate(Math.sin, 0, Math.PI)).toBeCloseTo(2, 9);
    expect(kit.integrate((x) => x * x, 0, 3)).toBeCloseTo(9, 9);
  });

  it('follows a system with RK4, and keeps what it must keep', () => {
    const decay = kit.ode((s) => ({ y: -s.y }), { y: 1 }, 0, 1, { dt: 0.01 });
    expect(decay.at(-1)!.t).toBeCloseTo(1, 12);
    expect(decay.at(-1)!.y).toBeCloseTo(Math.exp(-1), 9);
    const b = 0.3, g = 0.1;
    const sir = kit.ode((s) => ({ S: -b * s.S * s.I, I: b * s.S * s.I - g * s.I, R: g * s.I }), { S: 0.99, I: 0.01, R: 0 }, 0, 160, { dt: 0.05 });
    for (const s of sir) expect(s.S + s.I + s.R).toBeCloseTo(1, 9);
  });

  it('keeps the energy of an oscillator with velocity Verlet', () => {
    const energy = (s: Record<string, number>) => 0.5 * s.v * s.v + 0.5 * s.x * s.x;
    const path = kit.ode((s) => ({ x: s.v, v: -s.x }), { x: 1, v: 0 }, 0, 20 * Math.PI, { dt: 0.01, method: 'verlet', pairs: { x: 'v' } });
    expect(Math.abs(energy(path.at(-1)!) - 0.5) / 0.5).toBeLessThan(1e-4);
  });

  it('runs a simulation with its controls and invariants', () => {
    const el = document.createElement('div');
    document.body.appendChild(el);
    const drawn: number[] = [];
    const run = kit.sim(el, {
      state: { x: 1, v: 0 },
      rates: (s: Record<string, number>) => ({ x: s.v, v: -s.x }),
      invariants: { 'cơ năng': (s: Record<string, number>) => 0.5 * s.v * s.v + 0.5 * s.x * s.x },
      draw: (s: Record<string, number>) => drawn.push(s.x),
      paused: true,
    });
    expect([...el.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Chạy', 'Đặt lại']);
    expect(drawn[0]).toBe(1);
    expect(el.textContent).toContain('cơ năng');
    expect(run.time()).toBe(0);
  });
});

/**
 * Steps and graphs (plan 2026-10-28, §2.4, PR Q6). The Dijkstra of 2026-10-27
 * showed "∞" in its caption while step 0's table held B = 4, and drew arrows
 * on an undirected graph.
 */
describe("the kit's steps and graphs", () => {
  const fresh = () => {
    const el = document.createElement('div');
    document.body.appendChild(el);
    return el;
  };

  it('shows steps worked out before, from step 0, with back and next only where they go', () => {
    const el = fresh();
    const seen: number[] = [];
    const steps = [{ line: 'Bắt đầu: d(A) = 0' }, { line: 'Chọn A: d(B) = min(∞, 0 + 4) = 4' }, { line: 'Xong' }];
    kit.stepper(el, { steps, render: (_s: unknown, i: number, stage: Element) => { seen.push(i); stage.textContent = `step ${i}`; } });
    const [back, next] = [...el.querySelectorAll('button')] as HTMLButtonElement[];
    expect(seen).toEqual([0]);
    expect(el.textContent).toContain('Bước 0 / 2');
    expect(el.textContent).toContain('Bắt đầu: d(A) = 0');
    expect(back.disabled).toBe(true);
    next.click();
    next.click();
    expect(seen).toEqual([0, 1, 2]);
    expect(el.textContent).toContain('Xong');
    expect(next.disabled).toBe(true);
    expect(back.disabled).toBe(false);
  });

  it('refuses to show no steps', () => {
    expect(() => kit.stepper(fresh(), { steps: [], render: () => {} })).toThrow(/steps/);
  });

  it('draws an undirected graph with no arrows, its weights on boxes, and its states named', () => {
    const el = fresh();
    kit.graph(el, {
      nodes: [{ id: 'A' }, { id: 'B' }, { id: 'C' }],
      edges: [{ from: 'A', to: 'B', weight: 4 }, { from: 'B', to: 'C', weight: 1.5 }],
      state: { nodes: { A: 'done', B: 'current' }, edges: { 'B-A': 'chosen' }, notes: { A: 'd = 0' } },
      legend: { done: 'đã chốt', current: 'đang xét', chosen: 'trên đường đi' },
    });
    expect(el.querySelectorAll('[marker-end]').length).toBe(0);
    expect(el.querySelectorAll('marker').length).toBe(0);
    const lines = [...el.querySelectorAll('line')];
    // Undirected: the edge A–B is chosen whichever way its state names it.
    expect(lines[0].getAttribute('stroke')).toBe('var(--accent)');
    expect(lines[1].getAttribute('stroke')).toBe('var(--muted)');
    const label = [...el.querySelectorAll('text')].find((t) => t.textContent === '4')!;
    expect(label.previousElementSibling?.tagName).toBe('rect');
    expect([...el.querySelectorAll('text')].map((t) => t.textContent)).toEqual(expect.arrayContaining(['1,5', 'd = 0']));
    // A node's colour is its state; its letter is in its style, which kit.css cannot override.
    const circles = [...el.querySelectorAll('circle')];
    expect(circles[1].getAttribute('fill')).toBe('var(--accent)');
    const letter = [...el.querySelectorAll('text')].find((t) => t.textContent === 'B')!;
    expect(letter.style.fill).toBe('var(--surface)');
    expect(el.textContent!.indexOf('đã chốt')).toBeLessThan(el.textContent!.indexOf('đang xét'));
  });

  it('draws arrows on a directed graph, and places nodes with no positions round a circle', () => {
    const el = fresh();
    kit.graph(el, { nodes: [{ id: 'A' }, { id: 'B' }, { id: 'C' }, { id: 'D' }], edges: [{ from: 'A', to: 'B' }], directed: true });
    expect(el.querySelector('line')!.getAttribute('marker-end')).toBe('url(#arrow-idle)');
    const spots = [...el.querySelectorAll('circle')].map((c) => `${c.getAttribute('cx')},${c.getAttribute('cy')}`);
    expect(new Set(spots).size).toBe(4);
  });
});
