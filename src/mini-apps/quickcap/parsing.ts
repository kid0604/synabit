/**
 * QuickCap's tag grammar, in one place so it can be tested.
 *
 * This lived inline in `QuickCapApp.vue` as three regexes applied in
 * sequence. It is out here now because every rule in it decides what
 * happens to a user's own words, and that is not something to change by
 * eye.
 *
 * Two shapes are accepted, both of which the app has always written:
 *
 *   #tag             a single word
 *   #nhiều chữ#      Bear's wrapped form, for a tag containing spaces
 *
 * Both must begin with a letter. That rule alone removes `#404`, `#1`,
 * `#2026` and `#1a1a1c` — an issue number, a heading level, a year and
 * half the hex colours people write.
 *
 * It does not remove `#fff` or `#ff0000`, because `f` is a letter. Those
 * need the second rule: a run of exactly 3, 4, 6 or 8 hex digits is a CSS
 * colour, not a tag. The cost is real and worth naming — `#cafe`, `#face`
 * and `#facade` are words as well as colours, and this reads them as
 * colours. A tag with a diacritic, a hyphen or any other length is
 * unaffected, which covers essentially every tag this app is written for.
 *
 * Code is masked before scanning. `#include`, `#define` and `#!/bin/sh`
 * are not tags; QuickCap gained code blocks in 0.9.7, so this stopped
 * being hypothetical.
 *
 * Nothing here rewrites the body. Reading a tag out of a note and moving
 * it are different acts, and only the first one is this module's job.
 */

import colourContract from '../../../contracts/quickcap-colours.json';

/**
 * The card colours QuickCap offers, read from the contract the migration
 * also reads. Storing a name rather than a class string is what keeps a
 * styling detail out of the user's Markdown.
 */
export const CAP_COLOURS = colourContract.colours;

/**
 * The Tailwind classes for a stored colour.
 *
 * Deliberately tolerant: a cap that has not been migrated yet still has a
 * class string in `properties.color`, so an unrecognised value is returned
 * as-is and renders correctly. Reading both shapes is what lets a device
 * running the new build share a vault with one that is still on the old.
 */
export function colourClass(stored?: string): string {
  if (!stored) return '';
  return CAP_COLOURS.find((c) => c.name === stored)?.class ?? stored;
}

/**
 * 3, 4, 6 or 8 hex digits: the shapes CSS accepts for a colour.
 *
 * Checked after a candidate has already been matched, so it only ever
 * rejects — it can never pull in something the grammar did not allow.
 */
const HEX_COLOUR = /^(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** Characters allowed after a tag's opening letter. */
const TAG_TAIL = '[\\p{L}\\p{N}_-]*';

/**
 * `#word`.
 *
 * The trailing lookahead admits sentence punctuation so that
 * `họp về #dự-án.` still carries a tag; the punctuation itself is never
 * captured. Separators that ran off the end are trimmed afterwards
 * rather than excluded here, which keeps `#dự-án-` yielding `dự-án`
 * instead of yielding nothing at all.
 */
const SIMPLE_TAG = new RegExp(`(?:^|\\s)#(\\p{L}${TAG_TAIL})(?=[\\s.,;:!?)\\]]|$)`, 'gu');

/** `#nhiều chữ#` — Bear's wrapped form, spaces and all. */
const WRAPPED_TAG = new RegExp(`(?:^|\\s)#(\\p{L}[^#\\n]*?)#(?=[\\s.,;:!?)\\]]|$)`, 'gu');

/** Same length, no content — masking must not shift anything after it. */
const blank = (match: string) => ' '.repeat(match.length);

/**
 * Blank out anything the reader would see as code.
 *
 * Fences go first so that ``` is never mistaken for three inline spans.
 * The last rule catches a fence the user has opened and not yet closed,
 * which is the normal state of a note being typed.
 */
export function maskCode(content: string): string {
  return content
    .replace(/```[\s\S]*?```/g, blank)
    .replace(/~~~[\s\S]*?~~~/g, blank)
    .replace(/`[^`\n]*`/g, blank)
    .replace(/```[\s\S]*$/, blank);
}

/** Strip the legacy `<!--color:…-->` marker some older caps still carry. */
export function stripColorComment(content: string): string {
  return content.replace(/<!--color:.*?-->\n?/g, '');
}

/**
 * Every distinct tag in a cap, in the order it first appears.
 *
 * Wrapped tags are read first and masked out, so the closing hash of
 * `#nhiều chữ#` can never be picked up as the start of a second tag.
 */
export function extractTags(content: string): string[] {
  if (!content) return [];

  const scannable = maskCode(stripColorComment(content));
  const tags: string[] = [];

  const clean = (raw: string) => raw.trim().replace(/[-_]+$/, '');
  const isTag = (candidate: string) => candidate !== '' && !HEX_COLOUR.test(candidate);

  for (const match of scannable.matchAll(WRAPPED_TAG)) {
    const tag = clean(match[1]);
    if (isTag(tag)) tags.push(tag);
  }

  const withoutWrapped = scannable.replace(WRAPPED_TAG, blank);

  for (const match of withoutWrapped.matchAll(SIMPLE_TAG)) {
    const tag = clean(match[1]);
    if (isTag(tag)) tags.push(tag);
  }

  return Array.from(new Set(tags));
}

/**
 * Regex-safe form of a tag the user typed.
 *
 * `-` is deliberately absent: it is only special inside a character class,
 * and `\-` is an invalid escape under the `u` flag these patterns use.
 */
const escapeForRegex = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Remove one tag from a cap, leaving every other character alone.
 *
 * This is the one function here that does rewrite the body, because
 * deleting a tag is the user asking for exactly that. It removes the tag
 * and the whitespace that introduced it, then collapses only the blank
 * runs its own removal created — never the ones the user typed.
 */
export function removeTagFromContent(content: string, tag: string): string {
  const safe = escapeForRegex(tag);
  const wrapped = new RegExp(`(^|\\s)#${safe}#(?=[\\s.,;:!?)\\]]|$)`, 'gu');
  const simple = new RegExp(`(^|\\s)#${safe}(?=[\\s.,;:!?)\\]]|$)`, 'gu');

  return content
    .replace(wrapped, '')
    .replace(simple, '')
    .replace(/[ \t]+\n/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

/**
 * A cap's title: its first line with anything in it.
 *
 * FTS weights title ten times heavier than body (`bm25(…, 10.0, …, 1.0)`),
 * so a stale title points the strongest signal in search at text the user
 * deleted. The old rule froze the first fifty characters at creation and
 * never looked again.
 *
 * No migration goes with this. A cap that was never edited already has a
 * title matching its content, by construction — only editing could make one
 * stale, and editing is exactly what recomputes it here. The defect heals
 * on the caps a person actually touches, which are the only ones they will
 * ever search for.
 */
export function deriveTitle(content: string): string {
    const firstLine = stripColorComment(content)
        .split('\n')
        .map((line) => line.trim())
        .find((line) => line.length > 0);
    if (!firstLine) return 'Untitled';
    return firstLine.length > 80 ? `${firstLine.slice(0, 80).trimEnd()}…` : firstLine;
}
