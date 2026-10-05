/**
 * The geometry of an SVG path: read into absolute moves, lines and curves,
 * and flattened into points. Shared by the exporters, which redraw shapes in
 * other tools, and by a shape's connection handles, which sit where its
 * outline crosses its middle.
 */

export type Seg = { c: 'M' | 'L' | 'Q' | 'C'; p: number[] } | { c: 'Z'; p: [] };

/**
 * An SVG path as absolute moves, lines, curves and closes — every form a
 * path can be written in except arcs, which come back as null.
 */
export function parsePath(d: string): Seg[] | null {
  const tokens = d.match(/[a-zA-Z]|[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?/g) ?? [];
  const out: Seg[] = [];
  let i = 0;
  let cmd = '';
  let x = 0, y = 0, sx = 0, sy = 0;
  let lastCtrl: [number, number] | null = null;
  let lastKind = '';
  const n = () => Number(tokens[i++]);
  while (i < tokens.length) {
    if (/[a-zA-Z]/.test(tokens[i])) cmd = tokens[i++];
    else if (!cmd) return null;
    const rel = cmd === cmd.toLowerCase();
    const C = cmd.toUpperCase();
    const ox = rel ? x : 0, oy = rel ? y : 0;
    switch (C) {
      case 'M': x = ox + n(); y = oy + n(); sx = x; sy = y; out.push({ c: 'M', p: [x, y] }); cmd = rel ? 'l' : 'L'; lastKind = 'M'; continue;
      case 'L': x = ox + n(); y = oy + n(); out.push({ c: 'L', p: [x, y] }); break;
      case 'H': x = ox + n(); out.push({ c: 'L', p: [x, y] }); break;
      case 'V': y = oy + n(); out.push({ c: 'L', p: [x, y] }); break;
      case 'C': {
        const p = [ox + n(), oy + n(), ox + n(), oy + n(), ox + n(), oy + n()];
        out.push({ c: 'C', p }); lastCtrl = [p[2], p[3]]; x = p[4]; y = p[5]; lastKind = 'C'; continue;
      }
      case 'S': {
        const [cx, cy]: [number, number] = lastKind === 'C' && lastCtrl ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y];
        const p: number[] = [cx, cy, ox + n(), oy + n(), ox + n(), oy + n()];
        out.push({ c: 'C', p }); lastCtrl = [p[2], p[3]]; x = p[4]; y = p[5]; lastKind = 'C'; continue;
      }
      case 'Q': {
        const p = [ox + n(), oy + n(), ox + n(), oy + n()];
        out.push({ c: 'Q', p }); lastCtrl = [p[0], p[1]]; x = p[2]; y = p[3]; lastKind = 'Q'; continue;
      }
      case 'T': {
        const [cx, cy]: [number, number] = lastKind === 'Q' && lastCtrl ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y];
        const p: number[] = [cx, cy, ox + n(), oy + n()];
        out.push({ c: 'Q', p }); lastCtrl = [cx, cy]; x = p[2]; y = p[3]; lastKind = 'Q'; continue;
      }
      case 'Z': out.push({ c: 'Z', p: [] }); x = sx; y = sy; break;
      default: return null;
    }
    lastKind = C;
    lastCtrl = null;
  }
  return out.some((s) => s.p.some((v) => !Number.isFinite(v))) ? null : out;
}

/** The path as point lists, one per subpath, curves sampled; `closed` when it ends with Z. */
export function samplePath(segs: Seg[], steps = 8): { points: [number, number][]; closed: boolean }[] {
  const out: { points: [number, number][]; closed: boolean }[] = [];
  let cur: { points: [number, number][]; closed: boolean } | null = null;
  let at: [number, number] = [0, 0];
  for (const s of segs) {
    if (s.c === 'M') {
      cur = { points: [[s.p[0], s.p[1]]], closed: false };
      out.push(cur);
      at = [s.p[0], s.p[1]];
      continue;
    }
    if (!cur) {
      cur = { points: [at], closed: false };
      out.push(cur);
    }
    if (s.c === 'L') {
      at = [s.p[0], s.p[1]];
      cur.points.push(at);
    } else if (s.c === 'Q' || s.c === 'C') {
      const [x0, y0] = at;
      for (let k = 1; k <= steps; k++) {
        const t = k / steps, u = 1 - t;
        cur.points.push(s.c === 'Q'
          ? [u * u * x0 + 2 * u * t * s.p[0] + t * t * s.p[2], u * u * y0 + 2 * u * t * s.p[1] + t * t * s.p[3]]
          : [u * u * u * x0 + 3 * u * u * t * s.p[0] + 3 * u * t * t * s.p[2] + t * t * t * s.p[4],
              u * u * u * y0 + 3 * u * u * t * s.p[1] + 3 * u * t * t * s.p[3] + t * t * t * s.p[5]]);
      }
      at = s.c === 'Q' ? [s.p[2], s.p[3]] : [s.p[4], s.p[5]];
    } else if (s.c === 'Z') {
      cur.closed = true;
      at = cur.points[0];
      cur = null;
    }
  }
  return out.filter((sp) => sp.points.length > 1);
}

/**
 * Where an outline crosses the middle of its box, as insets in percent of the
 * 2–98 drawing area: the top and bottom where it crosses x = 50, the left and
 * right where it crosses y = 50. A side the outline never crosses (a pair of
 * brackets has nothing in the middle) falls back to the edge of what is drawn.
 */
export function outlineInsets(d: string): { top: number; right: number; bottom: number; left: number } {
  const edge = { top: 0, right: 0, bottom: 0, left: 0 };
  const segs = parsePath(d);
  if (!segs) return edge;
  const subpaths = samplePath(segs, 16);
  let top = Infinity, bottom = -Infinity, left = Infinity, right = -Infinity;
  let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
  for (const { points, closed } of subpaths) {
    const pts = closed ? [...points, points[0]] : points;
    for (let i = 0; i < pts.length; i++) {
      const [x1, y1] = pts[i];
      minX = Math.min(minX, x1); maxX = Math.max(maxX, x1); minY = Math.min(minY, y1); maxY = Math.max(maxY, y1);
      if (i === 0) continue;
      const [x0, y0] = pts[i - 1];
      if ((x0 - 50) * (x1 - 50) <= 0 && x0 !== x1) {
        const y = y0 + ((50 - x0) / (x1 - x0)) * (y1 - y0);
        top = Math.min(top, y); bottom = Math.max(bottom, y);
      } else if (x0 === 50 && x1 === 50) {
        top = Math.min(top, y0, y1); bottom = Math.max(bottom, y0, y1);
      }
      if ((y0 - 50) * (y1 - 50) <= 0 && y0 !== y1) {
        const x = x0 + ((50 - y0) / (y1 - y0)) * (x1 - x0);
        left = Math.min(left, x); right = Math.max(right, x);
      } else if (y0 === 50 && y1 === 50) {
        left = Math.min(left, x0, x1); right = Math.max(right, x0, x1);
      }
    }
  }
  if (!Number.isFinite(minX)) return edge;
  if (!Number.isFinite(top)) { top = minY; bottom = maxY; }
  if (!Number.isFinite(left)) { left = minX; right = maxX; }
  const pct = (v: number) => Math.max(0, (v / 96) * 100);
  return { top: pct(top - 2), bottom: pct(98 - bottom), left: pct(left - 2), right: pct(98 - right) };
}
