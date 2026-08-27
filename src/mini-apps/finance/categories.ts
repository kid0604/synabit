/**
 * Categories, and the one thing that used to be impossible to do to them.
 *
 * A category was a string in a list, and a transaction named the one it
 * belonged to by that string. So renaming a category was not a rename at all —
 * it removed one and added another, and every transaction filed under the old
 * name became unreachable: absent from every breakdown, still present in every
 * total.
 *
 * A category is now `{ id, name }`. The id of every category that already
 * existed **is its old name**, chosen for exactly one reason: transactions
 * already hold that string, so not one of them had to be rewritten. Renaming
 * changes `name` and leaves `id` alone, and the history follows.
 */

import type { Category } from './types';

/**
 * A stored category list, whichever of the two shapes it is in.
 *
 * A vault the storage repair has not reached still holds bare strings, and a
 * list of strings read as a list of objects renders as a column of blanks. This
 * is the same safety net `schema.ts` provides for amounts.
 */
export function toCategories(raw: unknown): Category[] {
  if (!Array.isArray(raw)) return [];

  return raw.flatMap((entry): Category[] => {
    if (typeof entry === 'string') {
      return entry ? [{ id: entry, name: entry }] : [];
    }
    if (entry && typeof entry === 'object') {
      const { id, name } = entry as { id?: unknown; name?: unknown };
      if (typeof id === 'string' && id) {
        return [{ id, name: typeof name === 'string' && name ? name : id }];
      }
    }
    return [];
  });
}

/**
 * What to call a category on screen.
 *
 * Falls back to the id, which for anything that predates ids is the name it
 * was given — so a transaction whose category has since been deleted still
 * reads as what the user filed it under rather than as nothing.
 */
export function categoryName(categories: Category[], id: string): string {
  return categories.find((c) => c.id === id)?.name ?? id;
}

/** Whether a name is already taken, ignoring case and surrounding space. */
export function nameIsTaken(categories: Category[], name: string, exceptId?: string): boolean {
  const wanted = name.trim().toLowerCase();
  return categories.some((c) => c.id !== exceptId && c.name.trim().toLowerCase() === wanted);
}

/**
 * An id for a category being created now.
 *
 * Minted rather than derived from the name, because the name is the one thing
 * about a category that is allowed to change. Only ever runs on the device
 * where somebody typed the name, so it needs no agreement with anywhere else —
 * unlike the migration, which had to be deterministic.
 */
export function newCategoryId(): string {
  return `cat-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
}
