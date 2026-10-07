/**
 * Colouring rules (design §7.5): a row or its cells tinted when a condition
 * holds, or a number column shaded from low to high. The first rule to colour
 * a row — or a cell — wins, as in a spreadsheet's conditional formatting.
 *
 * A rule whose condition does not compile colours nothing, rather than taking
 * the table down: it is shown as broken in the view's settings.
 */
import { compile, run, toBool, Problem } from '../formula';
import { schemaOf, valueOf } from './formulas';
import { numberOf, OPTION_COLORS, type RichTable, type Rule, type View } from './model';

export interface RowStyle {
  /** A tint for the whole row, as CSS. */
  row?: string;
  /** Tints for single cells, by column index, as CSS. */
  cells?: Record<number, string>;
}

const tint = (name: string | undefined) =>
  name && (OPTION_COLORS as readonly string[]).includes(name) ? `var(--rt-tint-${name})` : null;

/** Why a rule cannot run, or `null` when it can. */
export function ruleProblem(table: RichTable, rule: Rule): Problem | 'no_column' | null {
  if (rule.scale) {
    return table.columns.some((c) => c.name === rule.column) ? null : 'no_column';
  }
  if (!rule.when?.trim()) return null;
  const compiled = compile(rule.when, schemaOf(table));
  return compiled instanceof Problem ? compiled : null;
}

/**
 * The tints of the given rows under a view's rules. Rows no rule touches are
 * absent from the map.
 */
export function ruleStyles(table: RichTable, view: View, rows: number[], now = new Date()): Map<number, RowStyle> {
  const out = new Map<number, RowStyle>();
  const rules = view.rules ?? [];
  if (!rules.length || !rows.length) return out;
  const index = new Map(table.columns.map((c, i) => [c.name, i]));
  const schema = schemaOf(table);
  // A whole column read once, however many rules and rows ask for it.
  const wholes = new Map<string, ReturnType<typeof cellValue>[]>();
  const source = {
    rows: table.rows.length,
    now,
    memo: new Map(),
    cell: (column: string, row: number) => {
      const i = index.get(column)!;
      return cellValue(table, i, row);
    },
    column: (column: string) => {
      let values = wholes.get(column);
      if (!values) {
        values = table.rows.map((_, r) => cellValue(table, index.get(column)!, r));
        wholes.set(column, values);
      }
      return values;
    },
    other: () => [],
  };

  const style = (r: number) => {
    let s = out.get(r);
    if (!s) {
      s = {};
      out.set(r, s);
    }
    return s;
  };

  for (const rule of rules) {
    if (rule.scale) {
      const at = index.get(rule.column ?? '');
      const [lo, hi] = rule.scale;
      const from = tint(lo);
      const to = tint(hi);
      if (at === undefined || !from || !to) continue;
      const values = rows.map((r) => numberOf(table.rows[r][at] ?? ''));
      const xs = values.filter((v): v is number => v !== null);
      if (!xs.length) continue;
      let min = Infinity;
      let max = -Infinity;
      for (const x of xs) {
        if (x < min) min = x;
        if (x > max) max = x;
      }
      rows.forEach((r, k) => {
        const v = values[k];
        if (v === null) return;
        const s = style(r);
        s.cells ??= {};
        if (s.cells[at] !== undefined) return;
        const t = max === min ? 1 : (v - min) / (max - min);
        s.cells[at] = `color-mix(in srgb, ${to} ${Math.round(t * 100)}%, ${from})`;
      });
      continue;
    }
    const color = tint(rule.style?.row ?? rule.style?.cell);
    if (!color || !rule.when?.trim()) continue;
    const compiled = compile(rule.when, schema);
    if (compiled instanceof Problem) continue;
    const target = rule.style?.row ? 'row' : 'cell';
    const cells = target === 'cell'
      ? (rule.columns?.length ? rule.columns : [...compiled.deps.row])
        .map((n) => index.get(n)).filter((i): i is number => i !== undefined)
      : [];
    for (const r of rows) {
      let hit: boolean;
      try {
        hit = toBool(run(compiled, source, r)) === true;
      } catch {
        hit = false;
      }
      if (!hit) continue;
      const s = style(r);
      if (target === 'row') {
        s.row ??= color;
      } else {
        s.cells ??= {};
        for (const c of cells) s.cells[c] ??= color;
      }
    }
  }
  return out;
}

/** The style of a row as one string, so a memoised row re-renders only when its tint changes. */
export function styleKey(s: RowStyle | undefined): string {
  return s ? JSON.stringify(s) : '';
}

function cellValue(table: RichTable, column: number, row: number) {
  return valueOf(table.columns[column], table.rows[row]?.[column] ?? '');
}
