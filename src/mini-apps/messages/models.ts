/**
 * Turning a list of models into something a person can choose from.
 *
 * # What was wrong
 *
 * The picker drew every model the endpoint reported, in the order it reported
 * them, with no search. Against Ollama that is four or five rows and fine.
 * Against an OpenAI-compatible endpoint it is fifty to a hundred — and finding
 * one by scrolling a 64px-tall box is not a design, it is an absence of one.
 *
 * Two further things made it worse than long:
 *
 * * **It listed models that cannot answer a chat request at all.** `gpt-image-1`
 *   generates pictures, `gpt-4o-transcribe-diarize` reads audio, an embedding
 *   model returns vectors. Picking one is not a slow path — it is an error, and
 *   the list gave no hint. Truthful about what the endpoint said, useless as a
 *   thing to choose from.
 * * **Every row said `0 MB`.** Size is what Ollama reports for weights it hosts.
 *   A hosted endpoint has no such number and sends zero, so the one piece of
 *   metadata on every row was a lie about all of them.
 *
 * # Why the rules are patterns on names
 *
 * Because there is nothing else. The `/models` response carries an id and
 * little more; no capability, no modality. So this reads names — and errs
 * towards **showing**, because a chat model wrongly hidden is a model somebody
 * cannot reach at all, while a non-chat model wrongly shown costs one failed
 * request. Nothing is ever hidden outright: the count is on screen and one
 * click brings them all back.
 */
import type { ModelInfo } from './types';

/**
 * Names that mean "this does something other than chat".
 *
 * Deliberately short and deliberately unambiguous. `audio` and `realtime` are
 * *not* here even though both are awkward on `/chat/completions`: they are
 * near enough to chat that hiding them would be this module deciding something
 * it cannot know from a string.
 */
const NOT_CHAT = [
  'embed',
  'image',
  'dall-e',
  'tts',
  'whisper',
  'transcribe',
  'diarize',
  'moderation',
  'rerank',
];

/** Whether this model would fail a chat request outright. */
export const cannotChat = (name: string): boolean => {
  const lower = name.toLowerCase();
  return NOT_CHAT.some(word => lower.includes(word));
};

/**
 * Whether a name matches what somebody is typing.
 *
 * Every typed character has to appear, in order, but not adjacently — so `41n`
 * finds `gpt-4.1-nano` and `o4dr` finds `o4-mini-deep-research`. Punctuation in
 * the query is ignored, because nobody types `gpt-4.1-` and the dashes and dots
 * in these names are noise to a person hunting for one.
 */
export const matches = (name: string, query: string): boolean => {
  const needle = query.toLowerCase().replace(/[^a-z0-9]/g, '');
  if (!needle) return true;
  const hay = name.toLowerCase();
  let at = 0;
  for (const ch of needle) {
    at = hay.indexOf(ch, at);
    if (at === -1) return false;
    at += 1;
  }
  return true;
};

/**
 * How well it matches, for ordering. Lower is better.
 *
 * A person typing `gpt-4` wants `gpt-4` itself above `gpt-4o-transcribe`, and a
 * subsequence match is the last resort rather than the whole ranking — without
 * this, `41n` puts every name containing those letters somewhere in between in
 * whatever order the endpoint happened to use.
 */
const rank = (name: string, query: string): number => {
  const q = query.toLowerCase().trim();
  if (!q) return 4;
  const lower = name.toLowerCase();
  if (lower === q) return 0;
  if (lower.startsWith(q)) return 1;
  if (lower.includes(q)) return 2;

  // Again with the punctuation gone, because a person typing `41n` means
  // `gpt-4.1-nano` and not `gpt-4.1-mini` — and against the raw string neither
  // contains it, so both fall to the bottom together and alphabetical order
  // decides, which puts the wrong one first.
  const bare = (t: string) => t.replace(/[^a-z0-9]/g, '');
  if (bare(lower).includes(bare(q))) return 3;

  return 4;
};

export interface Shortlist {
  /** What to draw, in order. */
  shown: ModelInfo[];
  /** How many were left out for not being chat models. */
  hidden: number;
}

/**
 * The list to draw: filtered, ranked, with the current one first.
 *
 * The selected model leads whenever it still matches what is typed. Somebody
 * opening the picker to check what they are on should see it without reading,
 * and somebody switching away has it as the reference point.
 */
export const shortlist = (
  models: ModelInfo[],
  options: { query: string; selected: string; showAll: boolean }
): Shortlist => {
  const { query, selected, showAll } = options;

  // Never hide the one in use, whatever its name looks like. A rule that made
  // the current selection disappear from the list would be a rule nobody could
  // reason about.
  const usable = models.filter(m => showAll || !cannotChat(m.name) || m.name === selected);
  const hidden = models.length - usable.length;

  const shown = usable
    .filter(m => matches(m.name, query))
    .sort((a, b) => {
      if (a.name === selected) return -1;
      if (b.name === selected) return 1;
      const byRank = rank(a.name, query) - rank(b.name, query);
      if (byRank !== 0) return byRank;
      return a.name.localeCompare(b.name);
    });

  return { shown, hidden };
};

/**
 * The size, or nothing at all.
 *
 * A hosted endpoint reports no size and sends zero, and `0 MB` under every row
 * is worse than a blank one: it reads as a measurement.
 */
export const sizeLabel = (model: ModelInfo, format: (bytes: number) => string): string =>
  model.size > 0 ? format(model.size) : '';
