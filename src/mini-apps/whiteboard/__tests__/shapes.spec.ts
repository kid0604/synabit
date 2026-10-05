import { describe, it, expect } from 'vitest';
import { SHAPES, SHAPES_MAP } from '../shapes';

const PATH_CHARS = /^[MLCQZHVA0-9.,\s-]+$/;
const numbersOf = (d: string) => (d.match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);

describe('shape library', () => {
  it('has unique ids, and every id is in the map', () => {
    const ids = SHAPES.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) expect(SHAPES_MAP[id]?.id).toBe(id);
  });

  it('names each shape by its own i18n key', () => {
    for (const s of SHAPES) expect(s.labelKey).toBe(`whiteboard.shape_names.${s.id}`);
  });

  it('draws inside the 100×100 box with plain path syntax', () => {
    for (const s of SHAPES) {
      for (const d of [s.path, ...(s.deco ?? [])]) {
        expect(d, s.id).toMatch(PATH_CHARS);
        expect(d.trim().startsWith('M'), s.id).toBe(true);
        for (const v of numbersOf(d)) {
          expect(v, `${s.id}: ${v}`).toBeGreaterThanOrEqual(-5);
          expect(v, `${s.id}: ${v}`).toBeLessThanOrEqual(105);
        }
      }
    }
  });

  it('keeps the outline free of arcs, so handle placement can read it as x,y pairs', () => {
    for (const s of SHAPES) {
      expect(s.path, s.id).not.toMatch(/A/);
      expect(numbersOf(s.path).length % 2, s.id).toBe(0);
    }
  });

  it('gives every shape a usable default size', () => {
    for (const s of SHAPES) {
      expect(s.defaultWidth, s.id).toBeGreaterThan(0);
      expect(s.defaultHeight, s.id).toBeGreaterThan(0);
    }
  });
});
