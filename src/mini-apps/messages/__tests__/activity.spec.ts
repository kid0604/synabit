import { describe, it, expect } from 'vitest';
import { groupRuns, needsYou, RECENT_MS } from '../activity';
import type { RunSummary } from '../types';

const NOW = Date.parse('2026-09-26T12:00:00Z');
const run = (id: string, state: RunSummary['state'], ago: number): RunSummary => ({
  id,
  goal: id,
  trigger: 'user',
  surface: 'app',
  state,
  step_count: 1,
  tool_calls: 0,
  created_at: new Date(NOW - ago).toISOString(),
  updated_at: new Date(NOW - ago).toISOString(),
});

describe("Syn's recent work", () => {
  it('asks the three questions a person has, in that order', () => {
    const a = groupRuns(
      [
        run('done-old', 'done', RECENT_MS + 1),
        run('done-new', 'done', 60_000),
        run('asked', 'awaiting_consent', 5_000),
        run('which', 'awaiting_choice', 1_000),
        run('going', 'working', 0),
        run('gave-up', 'budget_exhausted', 10_000),
      ],
      NOW,
    );
    expect(a.working.map(r => r.id)).toEqual(['going']);
    expect(a.waiting.map(r => r.id)).toEqual(['which', 'asked']);
    expect(a.finished.map(r => r.id)).toEqual(['gave-up', 'done-new']);
  });

  /** A run waiting a week for an answer is still waiting. */
  it('never lets something waiting age out', () => {
    const a = groupRuns([run('asked-long-ago', 'awaiting_consent', 7 * RECENT_MS)], NOW);
    expect(needsYou(a)).toBe(1);
  });
});
