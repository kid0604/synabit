import { humanizeKey } from '../fieldRegistry';

/** Just enough of a field row to render one; the panel's rows satisfy it. */
interface Row {
  key: string;
  value: string;
}

/**
 * A node's fields as a Markdown table, or nothing when there is no point.
 *
 * Exporting was written for notes, where the body is the whole of it. For a
 * `book` the body is a line and the substance is `author`, `rating`, `read_at`
 * — so the note exporter, ported faithfully, would write a title and a blank
 * page beneath it.
 *
 * `tags` is skipped because the export dialog already offers tags as its own
 * option, and skipping it means a note — which usually has nothing else —
 * produces no table at all, and exports through Things exactly as it does
 * through Notes.
 */
export function propertiesTable(rows: Row[]): string {
  const useful = rows.filter(r => r.key && r.key !== 'tags' && r.value !== '');
  if (!useful.length) return '';

  return [
    '| | |',
    '| --- | --- |',
    // A value holding a pipe would otherwise end the cell early and shift
    // every column after it.
    ...useful.map(r => `| ${humanizeKey(r.key)} | ${r.value.replace(/\|/g, '\\|')} |`),
  ].join('\n');
}

/** The table above the body, with neither forced on the other. */
export function withProperties(body: string, rows: Row[]): string {
  const table = propertiesTable(rows);
  if (!table) return body;
  return body ? `${table}\n\n${body}` : table;
}
