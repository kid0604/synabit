import { describe, it, expect } from 'vitest';

import { mayBeEnabled, orderSkills } from '../useSynSkills';
import type { Skill } from '../../types';

const skill = (over: Partial<Skill> & { name: string }): Skill => ({
  id: `SynSkills/${over.name}.md`,
  title: over.name,
  description: '',
  when_to_use: '',
  tier: 'prose',
  tools: [],
  version: 1,
  author: 'user',
  enabled: false,
  body: '',
  ...over,
});

describe('the order the skills screen reads in', () => {
  /**
   * Enabled first, because those are the ones changing behaviour right now and
   * somebody opening this screen is usually there to check on them or turn one
   * off. A disabled skill is inert; it can wait.
   */
  it('puts what is on above what is off', () => {
    const rows = [
      skill({ name: 'aaa-off', enabled: false }),
      skill({ name: 'zzz-on', enabled: true }),
    ];

    expect(orderSkills(rows).map(s => s.name)).toEqual(['zzz-on', 'aaa-off']);
  });

  /**
   * By name within a group, and not by anything that moves. Sorting by last
   * used would reshuffle the list under somebody's cursor every time Syn
   * opened a skill, which is how a screen becomes untrustworthy to click.
   */
  it('then by name, so the list does not move on its own', () => {
    const rows = [
      skill({ name: 'weekly-review', enabled: true }),
      skill({ name: 'inbox-zero', enabled: true }),
      skill({ name: 'archive-old', enabled: true }),
    ];

    expect(orderSkills(rows).map(s => s.name)).toEqual([
      'archive-old',
      'inbox-zero',
      'weekly-review',
    ]);
  });

  it('sorts a copy, leaving the caller its own list', () => {
    const rows = [skill({ name: 'b', enabled: true }), skill({ name: 'a', enabled: true })];
    const before = rows.map(s => s.name);

    orderSkills(rows);

    expect(rows.map(s => s.name)).toEqual(before);
  });
});

describe('who may turn a skill on', () => {
  /**
   * The roadmap's steps 3 and 4 say a skill Syn wrote must be tried and then
   * reviewed before it is enabled, and that neither may be skipped. This is the
   * half a rule can carry.
   *
   * Mirrors `Skill::may_be_enabled` in Rust. Two copies of one rule, and the
   * one that matters is the Rust one — this is the button being honest about
   * it, not the enforcement.
   */
  it('lets me enable what I wrote, and makes Syn earn it', () => {
    expect(mayBeEnabled(skill({ name: 'mine', author: 'user' }))).toBe(true);

    const untried = skill({ name: 'theirs', author: 'syn' });
    expect(mayBeEnabled(untried)).toBe(false);

    expect(mayBeEnabled({ ...untried, trial_at: '2026-09-04' })).toBe(true);
  });
});
