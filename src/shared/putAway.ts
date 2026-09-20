/**
 * Asking the app not to raise something again, from a lens answer.
 *
 * # Why this has to exist before the panels can go
 *
 * §9 of `docs/nexus-lenses-2026-09-19.md` promised the eight panels would
 * become eight lenses and the code would shrink. The questions did become
 * lenses. But three of those panels were never only questions:
 *
 * | panel | its gesture |
 * | --- | --- |
 * | Ngày này năm xưa | `timeline_not_again` — don't raise this moment again |
 * | Khoảng lặng | `timeline_set_aside` — leave this person be for now |
 * | Một năm bằng lời mình | `timeline_drop_line` — not this sentence |
 *
 * Those are **tier two**: a person's decisions, the layer the whole timeline
 * is allowed to exist because of. Deleting the panels without moving the
 * gesture would not have removed code, it would have removed somebody's way of
 * saying no.
 *
 * # It reads the answer, the way everything else here does
 *
 * There is no new command and no new Rust. What kind of refusal a row affords
 * is read off the **columns**, exactly as `shapeFor` reads the shape from
 * them: an answer with `quiet` in it is about people, one with `day/note/text`
 * is about sentences, one with a day and a note is about moments. A row whose
 * answer says none of those affords nothing, and the button is not drawn.
 */
import { invoke } from '@tauri-apps/api/core';
import type { QueryResult, QueryRow } from './views/types';
import { dateColumn } from './views/shapeFor';

/** What saying no to this row would mean. */
export type PutAway =
  | { kind: 'person'; who: string }
  | { kind: 'line'; node: string; text: string }
  | { kind: 'moment'; node: string; day: string };

const at = (result: QueryResult, name: string) => result.columns.indexOf(name);
const cell = (row: QueryRow, index: number) => (index >= 0 ? (row.cells[index] ?? '').trim() : '');

/**
 * What this row can be put away as, or nothing.
 *
 * Order is meaning, not preference. A `seq gaps` answer also has days in it,
 * and putting one of its rows away is about the **person**, not about the day
 * they were last seen.
 */
export function putAwayFor(result: QueryResult | null, row: QueryRow): PutAway | null {
  if (!result) return null;

  // `seq gaps by who` — the row is a person's whole thread.
  const who = at(result, 'who');
  if (who >= 0 && at(result, 'quiet') >= 0) {
    const name = cell(row, who);
    return name ? { kind: 'person', who: name } : null;
  }

  // `explode sentences` — the row is one sentence of one note.
  const note = at(result, 'note');
  const text = at(result, 'text');
  if (note >= 0 && text >= 0) {
    const [where, what] = [cell(row, note), cell(row, text)];
    return where && what ? { kind: 'line', node: where, text: what } : null;
  }

  // Anything with a day in it and a node behind it — one thing, on one day.
  const day = cell(row, dateColumn(result));
  const node = row.open ?? row.id;
  return day && node ? { kind: 'moment', node, day } : null;
}

/**
 * Write the refusal.
 *
 * Three commands rather than one, because they were already the right three:
 * each is a thin wrapper over `quiet::write_hush` with a different subject,
 * and none of them ever knew which panel was calling.
 */
export async function putAway(vaultPath: string, what: PutAway): Promise<void> {
  if (what.kind === 'person') {
    await invoke('timeline_set_aside', { vaultPath, who: what.who });
    return;
  }
  if (what.kind === 'line') {
    await invoke('timeline_drop_line', { vaultPath, nodeId: what.node, text: what.text });
    return;
  }
  await invoke('timeline_not_again', { vaultPath, nodeId: what.node, day: what.day });
}

/** What to call the gesture, so the button says what it will do. */
export function putAwayLabel(what: PutAway): string {
  return `nexus.put_away_${what.kind}`;
}
