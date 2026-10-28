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
