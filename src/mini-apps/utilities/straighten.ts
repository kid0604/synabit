/**
 * A photo of a sheet of paper made into a flat, upright scan of it: the four
 * corners of the sheet (found by Vision on a Mac, or set by hand) are pulled
 * out to a rectangle, and every pixel of the result looked up where it came
 * from in the photo.
 *
 * Done here rather than in Rust: it is a loop over pixels the WebView already
 * holds, it needs no codec, and it works the same off macOS, where the corners
 * are dragged into place by hand.
 */

/** A corner as shares of the picture, from its top left. */
export type Point = [number, number];
/** Top left, top right, bottom right, bottom left. */
export type Quad = [Point, Point, Point, Point];

/** The whole picture: what "do not straighten this one" means. */
export const WHOLE: Quad = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];

/**
 * The projective map from the unit square onto `q` (Heckbert, 1989): (0,0) to
 * its top left, (1,0) top right, (1,1) bottom right, (0,1) bottom left.
 */
export function squareToQuad(q: Quad): (u: number, v: number) => Point {
  const [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = q;
  const dx3 = x0 - x1 + x2 - x3;
  const dy3 = y0 - y1 + y2 - y3;
  let a: number, b: number, d: number, e: number, g: number, h: number;
  if (Math.abs(dx3) < 1e-12 && Math.abs(dy3) < 1e-12) {
    // A parallelogram: the map is affine.
    [a, b, d, e, g, h] = [x1 - x0, x3 - x0, y1 - y0, y3 - y0, 0, 0];
  } else {
    const [dx1, dx2, dy1, dy2] = [x1 - x2, x3 - x2, y1 - y2, y3 - y2];
    const den = dx1 * dy2 - dx2 * dy1 || 1e-12;
    g = (dx3 * dy2 - dx2 * dy3) / den;
    h = (dx1 * dy3 - dx3 * dy1) / den;
    a = x1 - x0 + g * x1;
    b = x3 - x0 + h * x3;
    d = y1 - y0 + g * y1;
    e = y3 - y0 + h * y3;
  }
  return (u, v) => {
    const w = g * u + h * v + 1;
    return [(a * u + b * v + x0) / w, (d * u + e * v + y0) / w];
  };
}

const distance = (p: Point, q: Point, w: number, h: number) => Math.hypot((p[0] - q[0]) * w, (p[1] - q[1]) * h);

/**
 * How large the flat page comes out, in pixels: its longer edges as they were
 * in the photo, the longer side kept to `most`.
 */
export function flatSize(q: Quad, width: number, height: number, most = 3000): [number, number] {
  const w = Math.max(distance(q[0], q[1], width, height), distance(q[3], q[2], width, height));
  const h = Math.max(distance(q[0], q[3], width, height), distance(q[1], q[2], width, height));
  const s = Math.min(1, most / Math.max(w, h, 1));
  return [Math.max(1, Math.round(w * s)), Math.max(1, Math.round(h * s))];
}

/**
 * The flat page's pixels, RGBA: each looked up in the photo through the map,
 * between its four nearest pixels.
 */
export function warp(src: Uint8ClampedArray | Uint8Array, sw: number, sh: number, q: Quad, ow: number, oh: number): Uint8ClampedArray {
  const map = squareToQuad(q);
  const out = new Uint8ClampedArray(ow * oh * 4);
  for (let y = 0; y < oh; y++) {
    const v = (y + 0.5) / oh;
    for (let x = 0; x < ow; x++) {
      const [px, py] = map((x + 0.5) / ow, v);
      const fx = Math.min(sw - 1, Math.max(0, px * sw - 0.5));
      const fy = Math.min(sh - 1, Math.max(0, py * sh - 0.5));
      const x0 = Math.floor(fx);
      const y0 = Math.floor(fy);
      const x1 = Math.min(sw - 1, x0 + 1);
      const y1 = Math.min(sh - 1, y0 + 1);
      const [tx, ty] = [fx - x0, fy - y0];
      const i00 = (y0 * sw + x0) * 4;
      const i10 = (y0 * sw + x1) * 4;
      const i01 = (y1 * sw + x0) * 4;
      const i11 = (y1 * sw + x1) * 4;
      const o = (y * ow + x) * 4;
      for (let c = 0; c < 4; c++) {
        const top = src[i00 + c] + (src[i10 + c] - src[i00 + c]) * tx;
        const bottom = src[i01 + c] + (src[i11 + c] - src[i01 + c]) * tx;
        out[o + c] = top + (bottom - top) * ty;
      }
    }
  }
  return out;
}

/**
 * A photo's page made to look scanned: the paper's shade lifted to white and
 * the ink darkened, from where most of the light and dark pixels fall — so a
 * page photographed in yellow lamplight comes out white.
 */
export function asScan(px: Uint8ClampedArray): void {
  const counts = new Uint32Array(256);
  const n = px.length / 4;
  for (let i = 0; i < px.length; i += 4) counts[(px[i] * 77 + px[i + 1] * 150 + px[i + 2] * 29) >> 8]++;
  const level = (share: number) => {
    let seen = 0;
    for (let v = 0; v < 256; v++) {
      seen += counts[v];
      if (seen >= n * share) return v;
    }
    return 255;
  };
  // The paper is most of a page: its shade is near the top of the light pixels.
  const black = level(0.02);
  const white = Math.max(black + 40, level(0.7));
  // The paper's own colour, each channel, so a yellow cast goes too.
  const paper = [0, 0, 0];
  let papers = 0;
  for (let i = 0; i < px.length; i += 4) {
    if ((px[i] * 77 + px[i + 1] * 150 + px[i + 2] * 29) >> 8 >= white) {
      paper[0] += px[i];
      paper[1] += px[i + 1];
      paper[2] += px[i + 2];
      papers++;
    }
  }
  const scale = paper.map((sum) => 255 / Math.max(40, (papers ? sum / papers : white) - black));
  for (let i = 0; i < px.length; i += 4) {
    for (let c = 0; c < 3; c++) px[i + c] = (px[i + c] - black) * scale[c];
  }
}
