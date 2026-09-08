import { describe, it, expect } from 'vitest';

import { cannotChat, matches, shortlist, sizeLabel } from '../models';
import type { ModelInfo } from '../types';
import selector from '../components/ModelSelector.vue?raw';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

const model = (name: string, size = 0): ModelInfo => ({
  name,
  model: name,
  size,
  digest: '',
  modified_at: '',
});

/** What a hosted endpoint actually returns — the screenshot this came from. */
const hosted = [
  'gpt-4.1-nano-2025-04-14',
  'gpt-4.1-nano',
  'gpt-image-1',
  'o4-mini-deep-research',
  'gpt-4o-transcribe-diarize',
  'gpt-5.6-terra',
  'text-embedding-3-large',
].map(n => model(n));

/**
 * A list of a hundred models with no search is not a design.
 *
 * Against Ollama the picker showed four rows and was fine. Against an
 * OpenAI-compatible endpoint it showed everything the server reported, in the
 * server's order, and finding one meant dragging a scrollbar through a 64px
 * window.
 */
describe('finding a model', () => {
  it('matches letters in order, so a few keystrokes are enough', () => {
    expect(matches('gpt-4.1-nano', '41n')).toBe(true);
    expect(matches('o4-mini-deep-research', 'o4dr')).toBe(true);
    expect(matches('gpt-5.6-terra', 'terra')).toBe(true);
    expect(matches('gpt-4.1-nano', 'zzz')).toBe(false);
  });

  /** Nobody types the dots and dashes in these names. */
  it('ignores punctuation in what is typed', () => {
    expect(matches('gpt-4.1-nano', 'gpt41')).toBe(true);
    expect(matches('gpt-4.1-nano', 'gpt-4.1')).toBe(true);
  });

  it('shows everything when nothing is typed', () => {
    expect(matches('anything at all', '')).toBe(true);
  });

  /**
   * A subsequence match alone would scatter the obvious answer somewhere in the
   * middle, in whatever order the endpoint happened to use.
   */
  it('puts the closest name first', () => {
    const { shown } = shortlist(hosted, { query: 'gpt-4.1-nano', selected: '', showAll: true });
    expect(shown[0].name).toBe('gpt-4.1-nano');
  });

  /**
   * `41n` means `gpt-4.1-nano`. Against the raw strings neither it nor
   * `gpt-4.1-mini` contains it, so both land at the bottom together and
   * alphabetical order puts the wrong one on top — which is what the screen
   * showed the first time it was looked at.
   */
  it('ranks a punctuation-free match above an accidental one', () => {
    const models = ['gpt-4.1-mini', 'gpt-4.1-nano'].map(n => model(n));
    const { shown } = shortlist(models, { query: '41n', selected: '', showAll: true });
    expect(shown.map(m => m.name)).toEqual(['gpt-4.1-nano', 'gpt-4.1-mini']);
  });
});

/**
 * The list contained models that cannot answer a chat request at all.
 * `gpt-image-1` draws pictures; `gpt-4o-transcribe-diarize` reads audio.
 * Choosing one is not a slow path, it is an error — and nothing said so.
 */
describe('models that cannot chat', () => {
  it('recognises the ones that do something else', () => {
    for (const name of [
      'gpt-image-1',
      'dall-e-3',
      'text-embedding-3-large',
      'whisper-1',
      'gpt-4o-transcribe-diarize',
      'tts-1-hd',
      'omni-moderation-latest',
    ]) {
      expect(cannotChat(name), name).toBe(true);
    }
  });

  /**
   * The rule errs towards showing. A chat model wrongly hidden cannot be
   * reached at all; a non-chat model wrongly shown costs one failed request.
   */
  it('leaves the ambiguous ones alone', () => {
    for (const name of [
      'gpt-4.1-nano',
      'gpt-5.6-terra',
      'o4-mini-deep-research',
      'gpt-4o-audio-preview',
      'gpt-4o-realtime-preview',
      'llama3.2:3b',
    ]) {
      expect(cannotChat(name), name).toBe(false);
    }
  });

  it('keeps them out of the way but counts them', () => {
    const { shown, hidden } = shortlist(hosted, { query: '', selected: '', showAll: false });
    expect(hidden).toBe(3);
    expect(shown.map(m => m.name)).not.toContain('gpt-image-1');
    expect(shown.map(m => m.name)).toContain('gpt-4.1-nano');
  });

  /** Nothing is hidden outright: the rule is a guess about a string, and a
   *  guess nobody can undo is not one worth making. */
  it('brings them all back on request', () => {
    const { shown, hidden } = shortlist(hosted, { query: '', selected: '', showAll: true });
    expect(hidden).toBe(0);
    expect(shown).toHaveLength(hosted.length);
  });

  /** A rule that made the current selection vanish would be a rule nobody
   *  could reason about. */
  it('never hides the one in use', () => {
    const { shown } = shortlist(hosted, { query: '', selected: 'gpt-image-1', showAll: false });
    expect(shown[0].name).toBe('gpt-image-1');
  });
});

describe('what a row says', () => {
  /** Every row read `0 MB`. Size is what Ollama reports for weights it hosts;
   *  a hosted endpoint has none and sends zero. */
  it('says nothing rather than zero', () => {
    const format = (b: number) => `${b} B`;
    expect(sizeLabel(model('gpt-4.1-nano', 0), format)).toBe('');
    expect(sizeLabel(model('llama3.2:3b', 2048), format)).toBe('2048 B');
  });

  it('leads with the model in use, so checking takes no reading', () => {
    const { shown } = shortlist(hosted, { query: '', selected: 'gpt-5.6-terra', showAll: false });
    expect(shown[0].name).toBe('gpt-5.6-terra');
  });
});

describe('choosing without the mouse', () => {
  it('moves, picks and closes from the keyboard', () => {
    expect(selector).toContain("e.key === 'ArrowDown'");
    expect(selector).toContain("e.key === 'ArrowUp'");
    expect(selector).toContain("e.key === 'Enter'");
    expect(selector).toContain("e.key === 'Escape'");
  });

  /**
   * The composer and the run both listen for Escape. Closing a dropdown must
   * not also stop a stream.
   */
  it('does not let Escape reach the things behind it', () => {
    expect(selector).toContain('e.stopPropagation()');
  });

  it('focuses the box on opening, and forgets the last search', () => {
    expect(selector).toContain('search.value?.focus()');
    expect(selector).toContain("query.value = '';");
  });

  it('keeps the highlighted row in view', () => {
    expect(selector).toContain("scrollIntoView({ block: 'nearest' })");
  });

  it('has words for all of it, in both languages', () => {
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('model_search');
      expect(locale.syn).toHaveProperty('model_none_match');
      expect(locale.syn).toHaveProperty('model_hidden');
    }
  });
});
