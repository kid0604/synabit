import { describe, it, expect } from 'vitest';
import source from '../useSynModels.ts?raw';

/**
 * Which model Syn runs when nothing has been chosen.
 *
 * It took `result[0]`. Against Ollama that is harmless — the list is a handful
 * of chat models. Against an OpenAI-compatible endpoint the list is everything
 * the account can reach, and the first entry was `text-embedding-ada-002`: the
 * header said Syn was running an embedding model, every message sent to it
 * would have failed, and `Syn/settings.json` had said `gpt-5.6-luna` the whole
 * time.
 *
 * The heuristic is what is tested, because it is the part with an opinion in
 * it. Read out of the source so this cannot drift from a private function.
 */
describe('choosing a model when none is set', () => {
  const pattern = source
    .split('const isEmbeddingModel = (name: string): boolean =>')[1]
    ?.split('.test(name)')[0]
    ?.trim();

  const looksLikeEmbedding = (name: string) => {
    expect(pattern, 'useSynModels should still declare isEmbeddingModel').toBeTruthy();
    return new RegExp(pattern.slice(1, pattern.lastIndexOf('/')), 'i').test(name);
  };

  it('recognises the models that cannot hold a conversation', () => {
    for (const name of [
      'text-embedding-ada-002',
      'text-embedding-3-large',
      'whisper-1',
      'tts-1-hd',
      'dall-e-3',
      'omni-moderation-latest',
      'gpt-4o-audio-preview',
    ]) {
      expect(looksLikeEmbedding(name), `${name} should not be picked to chat with`).toBe(true);
    }
  });

  it('leaves the ones that can', () => {
    for (const name of [
      'gpt-5.6-luna',
      'gpt-4o',
      'o3-mini',
      'llama3.2',
      'gemma4:e4b',
      'qwen3:14b',
      'claude-opus-5',
    ]) {
      expect(looksLikeEmbedding(name), `${name} is a chat model`).toBe(false);
    }
  });

  it('prefers the vault default over any guess', () => {
    // Order matters and is easy to get backwards: the heuristic is a tie-break,
    // not a filter. Somebody who has chosen a model gets it even if its name
    // happens to match the pattern.
    expect(source).toMatch(/available\(preferred\)\s*\n?\s*\?/);
    expect(source.indexOf('syn_get_settings')).toBeLessThan(
      source.indexOf('isEmbeddingModel(m.name)'),
    );
  });
});
