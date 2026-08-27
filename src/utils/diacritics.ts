/**
 * Fold Vietnamese tone marks, the same way the search index folds them.
 *
 * Several lists filter locally for the moment before ranked results arrive
 * from SQLite — the note sidebar and the note manager both do, on a 200ms
 * debounce. Those interim filters compared raw strings, so typing `cong`
 * emptied the list for a fifth of a second and then filled it again once the
 * backend answered. Matching the backend's rules here removes the flicker.
 *
 * Matching them exactly is the point, and that now includes `đ`. SQLite's
 * `unicode61` leaves it alone — it is a letter, not a `d` with a mark — so the
 * index carries a shadow column of the `đ` words with it folded (see
 * `search_fold.rs` on the Rust side). Folding it here too keeps this pass and
 * the results that replace it agreeing; folding more, or less, would make the
 * list fill and then rearrange itself.
 */
export function foldDiacritics(text: string): string {
  return text
    .replace(/đ/g, 'd')
    .replace(/Đ/g, 'D')
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');
}

/** Case- and tone-insensitive containment, for those interim filters. */
export function looseIncludes(haystack: string, needle: string): boolean {
  return foldDiacritics(haystack.toLowerCase()).includes(foldDiacritics(needle.toLowerCase()));
}
