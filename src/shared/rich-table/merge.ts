/**
 * Undo, when someone else has changed the table since.
 *
 * An edit to a Rich Table is one step that sets the table's whole source,
 * so undoing it sets the whole old source back. If Syn or another device
 * added a row in between, that row went with it. Instead, the change undo
 * wants — from `base` (the source right after the edit) to `target` (the
 * source before it) — is worked out line by line and applied to `current`,
 * which keeps everything else that has happened.
 *
 * A change that no longer applies cleanly — the lines it touches were
 * changed from outside too — is left out rather than forced: undo then
 * keeps the other change, which is the side that loses nothing.
 */

interface Hunk {
  /** The lines of `base` replaced, as a range. */
  from: number;
  to: number;
  /** What they become. */
  lines: string[];
}

/** The longest common subsequence of two line lists, as matched index pairs. */
function lcs(a: string[], b: string[]): [number, number][] | null {
  // Lines common to both ends cost nothing; only the middle is compared.
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const n = endA - start;
  const m = endB - start;
  // Two thousand rows changed on both sides is past what an undo should
  // spend time on; the caller then keeps the current table.
  if (n * m > 4_000_000) return null;
  const width = m + 1;
  const table = new Uint16Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i * width + j] = a[start + i] === b[start + j]
        ? table[(i + 1) * width + j + 1] + 1
        : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  const pairs: [number, number][] = [];
  for (let k = 0; k < start; k++) pairs.push([k, k]);
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[start + i] === b[start + j]) {
      pairs.push([start + i, start + j]);
      i++;
      j++;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      i++;
    } else {
      j++;
    }
  }
  for (let k = 0; k < a.length - endA; k++) pairs.push([endA + k, endB + k]);
  return pairs;
}

/** How `to` differs from `from`, as hunks over `from`'s lines. */
function hunks(from: string[], to: string[]): Hunk[] | null {
  const pairs = lcs(from, to);
  if (!pairs) return null;
  const out: Hunk[] = [];
  let a = 0;
  let b = 0;
  for (const [i, j] of [...pairs, [from.length, to.length] as [number, number]]) {
    if (i > a || j > b) out.push({ from: a, to: i, lines: to.slice(b, j) });
    a = i + 1;
    b = j + 1;
  }
  return out;
}

/** Where `needle` sits in `hay` as consecutive lines, nearest `hint` when it sits in several places. */
function locate(hay: string[], needle: string[], hint: number): number {
  let best = -1;
  for (let i = 0; i + needle.length <= hay.length; i++) {
    let match = true;
    for (let k = 0; k < needle.length; k++) {
      if (hay[i + k] !== needle[k]) {
        match = false;
        break;
      }
    }
    if (match && (best === -1 || Math.abs(i - hint) < Math.abs(best - hint))) best = i;
  }
  return best;
}

/**
 * `current`, with the change from `base` to `target` applied to it.
 * `target` itself when nothing else has happened (`current === base`).
 */
export function rebase(base: string, target: string, current: string): string {
  if (current === base) return target;
  if (target === base) return current;
  const baseLines = base.split('\n');
  const changes = hunks(baseLines, target.split('\n'));
  if (!changes) return current;
  let lines = current.split('\n');
  // From the last change back, so earlier positions stay put.
  for (const h of [...changes].reverse()) {
    const removed = baseLines.slice(h.from, h.to);
    if (removed.length) {
      const at = locate(lines, removed, h.from);
      if (at === -1) continue;
      lines = [...lines.slice(0, at), ...h.lines, ...lines.slice(at + removed.length)];
    } else {
      // A pure insertion: after the line it followed, or before the one it preceded.
      const before = h.from > 0 ? locate(lines, [baseLines[h.from - 1]], h.from - 1) : -1;
      const after = h.from < baseLines.length ? locate(lines, [baseLines[h.from]], h.from) : -1;
      const at = before !== -1 ? before + 1 : after !== -1 ? after : -1;
      if (at === -1) continue;
      lines = [...lines.slice(0, at), ...h.lines, ...lines.slice(at)];
    }
  }
  return lines.join('\n');
}
