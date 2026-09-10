import { describe, it, expect } from 'vitest';

import source from '../ThingsApp.vue?raw';

/**
 * Every way Things opens a node goes past the PIN.
 *
 * What the lock does is covered in `useThingsLock.spec.ts`. What is left is
 * whether every door is wired to it, and a new door added without it is the
 * way this breaks again — so the guard is on the doors. `ThingsApp.vue` is
 * mounted nowhere in this suite; see `railStaysCurrent.spec.ts`.
 */
describe('the PIN stands in front of every way into a node', () => {
  const script = source.slice(0, source.indexOf('<template>'));
  const body = (name: string) => {
    const start = script.indexOf(`const ${name} = `);
    expect(start, `${name} is gone`).toBeGreaterThan(-1);
    const next = script.indexOf('\nconst ', start + 1);
    return script.slice(start, next === -1 ? undefined : next);
  };

  it.each(['openRow', 'openFromBody', 'openLinked', 'renameRow', 'duplicateRow', 'openHistory'])(
    '%s asks first',
    (name) => {
      expect(body(name)).toContain('nodeLock.whenUnlocked(');
    },
  );

  // Fetching is what shows a node. Only the unchecked opener does it, and the
  // restore, which is reached from a history that already took the PIN.
  it('fetches a node only in the opener the doors lead to', () => {
    const fetches = [...script.matchAll(/detail\.open\(/g)].map((m) => {
      const before = script.lastIndexOf('\nconst ', m.index);
      return script.slice(before + 7, script.indexOf(' ', before + 7));
    });
    expect(fetches.sort()).toEqual(['onVersionRestored', 'showNode']);
  });

  it('draws the PIN screen', () => {
    expect(source).toMatch(/<LockScreen[\s\S]*?v-if="nodeLock\.pending\.value"/);
  });

  it('hides a protected note’s text under the node it links to', () => {
    expect(source).toContain('nodeLock.hidesBody(bl.id)');
  });
});
