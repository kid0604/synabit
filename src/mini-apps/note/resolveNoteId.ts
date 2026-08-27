import type { NoteItem } from './helpers';

/**
 * Find the note a link, a deep link or a navigation request is asking for.
 *
 * The rule is: the path it names, then the file it names, and nothing looser.
 *
 * Looser is what used to happen. Matching any note whose path merely *ended*
 * with the target resolved `Notes/target.md` against an
 * `Archive/Notes/target.md` that happened to sit earlier in the list — so a
 * link opened a note the writer had never linked to, silently and plausibly
 * enough that nobody would think to check.
 *
 * The basename step stays because a target is not always a full path: older
 * links, deep links from outside the app and hand-written references may name
 * only the file. But it applies only when exactly one note carries that name.
 *
 * That last condition is the whole difference. With one `target.md` in the
 * vault, a link naming `Notes/target.md` is plainly about it wherever it has
 * since been moved to, and following it is right. With two, the folders are
 * the only thing telling them apart — so guessing between them is precisely
 * the thing not to do, and finding nothing is the honest answer.
 */
export function resolveNoteId(notes: NoteItem[], target: string): NoteItem | undefined {
  if (!target) return undefined;

  const exact = notes.find((n) => n.id === target);
  if (exact) return exact;

  const base = target.split('/').pop();
  if (!base) return undefined;

  const sharingTheName = notes.filter((n) => n.id.split('/').pop() === base);
  return sharingTheName.length === 1 ? sharingTheName[0] : undefined;
}
