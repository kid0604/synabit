/**
 * What shape an answer should be drawn in, read off the answer itself.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, §7 and step 4.
 *
 * # Why the answer chooses, and not the question
 *
 * This is what stops a new question costing new code. If the screen decided
 * how to draw things — a panel for anniversaries, a panel for counts, a panel
 * for meetings — then every question anybody thought of needed a panel, which
 * is exactly where the eight panels came from. If instead the *rows* say what
 * they are, one renderer covers every question anybody will ever ask in that
 * shape, including the ones nobody has thought of.
 *
 * # It reads values, not just names
 *
 * A column called `when` is a date. So is a column called `due_date`,
 * `completed_at`, or one a person invented this morning in their own schema
 * and filled with days. Deciding by name alone would work for the queries this
 * app writes and fail for the queries its users write, which is the wrong way
 * round.
 *
 * So names are a hint and the cells are the evidence. An empty answer has no
 * evidence, which is why names are consulted at all.
 */
import type { QueryResult } from './types';

/** Every way an answer can be drawn. `auto` means "you decide". */
export type Shape = 'auto' | 'dated' | 'bars' | 'table' | 'list';

/** Shapes a person can actually pick, in the order they are offered. */
export const SHAPES: Exclude<Shape, 'auto'>[] = ['dated', 'bars', 'list', 'table'];

/** Names that mean a day even before any cell has been seen. */
const SOUNDS_LIKE_A_DAY = ['when', 'from', 'date', 'day', 'happened_from'];

const A_DAY = /^\d{4}-\d{2}-\d{2}$/;

/**
 * What a `| stats` stage calls its column of numbers.
 *
 * Mirrors `Tally::column` in `query.rs`. A name **and** numbers in the cells,
 * both: `columns:title,priority` is also two columns with numbers in the
 * second, and it is a table of tasks, not a chart.
 */
const A_TALLY = ['count', 'sum', 'avg', 'min', 'max'];

/** Whether an answer is a heap of labels with a number against each. */
export function isTally(result: QueryResult): boolean {
  if (result.columns.length !== 2) return false;
  if (!A_TALLY.includes(result.columns[1].toLowerCase())) return false;
  const numbers = result.rows.map(row => (row.cells[1] ?? '').trim());
  return numbers.every(cell => cell !== '' && Number.isFinite(Number(cell)));
}

/** Enough of a column's cells being days to call it a date column. */
const MOSTLY = 0.8;

/**
 * Which column holds a day, or `-1`.
 *
 * The first such column wins, because a row with two dates — started and
 * finished — is placed on the first one, the way a timeline places a thing
 * where it began.
 */
export function dateColumn(result: QueryResult): number {
  const { columns, rows } = result;

  for (let at = 0; at < columns.length; at += 1) {
    const filled = rows.map(row => (row.cells[at] ?? '').trim()).filter(Boolean);
    if (filled.length) {
      if (filled.filter(cell => A_DAY.test(cell)).length >= filled.length * MOSTLY) return at;
      // Cells that are there and are not days settle it: the name does not
      // get to overrule what is actually in the column.
      continue;
    }
    // No cells to read — either no rows at all, or this column is empty.
    if (!rows.length && SOUNDS_LIKE_A_DAY.includes(columns[at].toLowerCase())) return at;
  }
  return -1;
}

/**
 * The shape to draw an answer in, when nobody has chosen one.
 *
 * Four shapes, each with something to draw. `bars` arrived with `| stats`,
 * which is the transform that produces its data — building it earlier would
 * have been guessing at what that data would look like. `quotes` is still
 * named in §7 and still waiting on `explode sentences`.
 */
export function shapeFor(result: QueryResult | null): Exclude<Shape, 'auto'> {
  if (!result) return 'list';
  // A count against a label is a comparison, and a column of digits is the one
  // way of showing a comparison that makes the reader do the comparing. Read
  // before the date test on purpose: `stats count by month` has days down one
  // side, and drawing it as a timeline would show the labels and hide the
  // thing that was counted.
  if (isTally(result)) return 'bars';
  // Anything with a day in it is a question about time, and time is the one
  // thing a table hides: the gaps between rows are the answer as much as the
  // rows are.
  if (dateColumn(result) >= 0) return 'dated';
  // Several columns means the columns were asked for, so show them.
  if (result.columns.length > 2) return 'table';
  return 'list';
}

/** The shape to use, honouring a choice when one was made. */
export function chosenShape(
  chosen: Shape | undefined,
  result: QueryResult | null,
): Exclude<Shape, 'auto'> {
  return !chosen || chosen === 'auto' ? shapeFor(result) : chosen;
}
