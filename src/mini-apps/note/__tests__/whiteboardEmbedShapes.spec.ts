import { describe, expect, it } from 'vitest';
import { shapeTransform, shapeLabelCenter } from '../nodes/WhiteboardNodeView.vue';
import { SHAPES_MAP } from '../../whiteboard/shapes';

/** Where a point of a shape path lands, through `translate(x,y) scale(sx,sy) translate(-2,-2)`. */
function place(transform: string, px: number, py: number) {
  const n = [...transform.matchAll(/-?[\d.]+/g)].map(Number);
  const [x, y, sx, sy, dx, dy] = n;
  return { x: x + sx * (px + dx), y: y + sy * (py + dy) };
}

describe('a shape in a board embedded in a note', () => {
  it('fills its item the way the board draws it: outline 2..98 to the edges', () => {
    const t = shapeTransform(100, 50, 192, 96);
    expect(place(t, 2, 2)).toEqual({ x: 100, y: 50 });
    expect(place(t, 98, 98)).toEqual({ x: 292, y: 146 });
  });

  it('puts a UML class name in its top compartment', () => {
    const def = SHAPES_MAP.umlClass;
    expect(def.labelBox).toBeDefined();
    const at = shapeLabelCenter(def, 0, 0, 180, 120);
    expect(at.x).toBe(90);
    // The top compartment is the top 29%; its middle is 14.5% down.
    expect(at.y).toBeCloseTo(120 * 0.145);
  });

  it('centres the words of a shape without a label box', () => {
    expect(shapeLabelCenter(SHAPES_MAP.rectangle, 10, 20, 160, 80)).toEqual({ x: 90, y: 60 });
    expect(shapeLabelCenter(undefined, 0, 0, 100, 50)).toEqual({ x: 50, y: 25 });
  });
});
