import { describe, it, expect } from 'vitest';
import router from '../../router';
import {
  nameForNodeType,
  folderForType,
  ROUTE_FOR_NODE_TYPE,
  routeForNodeType,
  nodeTypeFromPath,
  routeForNode,
} from '../nodeRoutes';

/**
 * These pin the bug this module exists for: a task reminder in Syn opened the
 * Notes editor on the task's own file, because the notification carried no
 * type and the reader treated "unknown" as "note". One autosave in that editor
 * would have rewritten the task as a note and lost it.
 */
describe('routeForNodeType', () => {
  it('sends each node type to the app that owns it', () => {
    expect(routeForNodeType('task')).toBe('task');
    expect(routeForNodeType('note')).toBe('note');
    expect(routeForNodeType('person')).toBe('person');
    expect(routeForNodeType('project')).toBe('project');
  });

  /** The two words differ, and every hand-rolled copy of this map got it right
   *  only by accident. */
  it('opens an event in the calendar', () => {
    expect(routeForNodeType('event')).toBe('calendar');
  });

  it('says it does not know rather than guessing', () => {
    for (const unknown of ['', null, undefined, 'habit', 'Task', 'TASK']) {
      expect(routeForNodeType(unknown as string), String(unknown)).toBeNull();
    }
  });

  /** The specific wrong answer that caused the bug. */
  it('never answers note for something that is not one', () => {
    for (const [type, route] of Object.entries(ROUTE_FOR_NODE_TYPE)) {
      if (type !== 'note') expect(route, type).not.toBe('note');
    }
    expect(routeForNodeType('something-new')).not.toBe('note');
  });
});

describe('nodeTypeFromPath', () => {
  it('reads the type off the folder', () => {
    expect(nodeTypeFromPath('Tasks/abc.md')).toBe('task');
    expect(nodeTypeFromPath('Events/abc.md')).toBe('event');
    expect(nodeTypeFromPath('People/abc.md')).toBe('person');
    expect(nodeTypeFromPath('Notes/abc.md')).toBe('note');
    expect(nodeTypeFromPath('Projects/abc.md')).toBe('project');
  });

  it('reads a Windows path the same way', () => {
    expect(nodeTypeFromPath('Tasks\\abc.md')).toBe('task');
  });

  it('handles a nested path', () => {
    expect(nodeTypeFromPath('Notes/work/q3/abc.md')).toBe('note');
  });

  it('does not invent a type for an unknown folder', () => {
    expect(nodeTypeFromPath('Archive/abc.md')).toBeNull();
    expect(nodeTypeFromPath('abc.md')).toBeNull();
    expect(nodeTypeFromPath('')).toBeNull();
    expect(nodeTypeFromPath(null)).toBeNull();
  });

  /** Folders are named exactly; a near-miss is not a match. */
  it('is not fooled by a similar folder name', () => {
    expect(nodeTypeFromPath('Task/abc.md')).toBeNull();
    expect(nodeTypeFromPath('tasks/abc.md')).toBeNull();
  });
});

describe('routeForNode', () => {
  it('prefers the type it was told over the path', () => {
    expect(routeForNode('note', 'Tasks/abc.md')).toBe('note');
  });

  /**
   * Notifications already written into a vault carry no `target_type`. The
   * path is the only thing left, and it is enough — this is what stops those
   * existing reminders from staying broken after the fix.
   */
  it('falls back to the path when no type was given', () => {
    expect(routeForNode(null, 'Tasks/abc.md')).toBe('task');
    expect(routeForNode(undefined, 'Events/abc.md')).toBe('calendar');
    expect(routeForNode('', 'People/abc.md')).toBe('person');
  });

  it('falls back to the path when the type means nothing', () => {
    expect(routeForNode('habit', 'Tasks/abc.md')).toBe('task');
  });

  it('gives up when neither identifies the node', () => {
    expect(routeForNode(null, 'Archive/abc.md')).toBeNull();
    expect(routeForNode(null, null)).toBeNull();
  });
});

/**
 * The folder map and the type map have to stay inverses of each other, and
 * both have to agree with Rust.
 *
 * `syn/tools.rs::folder_for_type` writes a thread into `SynThreads/`. This side
 * did not know that folder, so `folderForType('syn_thread')` answered
 * `Syn_thread` and `nodeTypeFromPath('SynThreads/x.md')` answered nothing —
 * two halves of one app disagreeing about where a file lives, which is the
 * exact failure the comment on `folderForType` is about.
 */
describe('where a thread lives', () => {
  it('agrees with the folder Rust writes it into', () => {
    expect(folderForType('syn_thread')).toBe('SynThreads');
    expect(nodeTypeFromPath('SynThreads/General.md')).toBe('syn_thread');
  });

  /** The prefixed kinds are filed apart so the plain words stay the user's. */
  it('files Syn\'s own kinds apart from the words they borrow', () => {
    expect(folderForType('syn_memory')).toBe('SynMemory');
    expect(folderForType('syn_skill')).toBe('SynSkills');
    // A user's own `thread` kind is untouched by any of it.
    expect(folderForType('thread')).toBe('Thread');
    expect(nodeTypeFromPath('Thread/mine.md')).toBeNull();
  });
});

/**
 * A name on screen only for the types this app invented.
 *
 * Things shows a type's own name on purpose, so that `animal` has a name at
 * all. This is the narrow exception — a prefix the user never chose — and it
 * has to keep the fallback, or that property is gone.
 */
describe('what a node type is called on screen', () => {
  it('names the kind whose prefix the user never chose', () => {
    expect(nameForNodeType('syn_thread')).toBe('Syn thread');
  });

  /**
   * Not shortened to `thread`. A rail showing `thread` twice would recreate
   * exactly the collision the prefix was invented to prevent.
   */
  it('stays distinguishable from a kind the user made', () => {
    expect(nameForNodeType('syn_thread')).not.toBe('thread');
    expect(nameForNodeType('thread')).toBe('thread');
  });

  it('leaves every other type exactly as it is written', () => {
    for (const raw of ['note', 'task', 'animal', 'réunion', 'cá', '']) {
      expect(nameForNodeType(raw), raw).toBe(raw);
    }
  });
});

/**
 * A thread opens in Messages, not in an app of its own.
 *
 * It briefly had one — a thirteenth sidebar entry for a feature one day old,
 * splitting one family across two entries while the conversations, the runs,
 * what Syn remembers and what it is allowed to do all lived in Messages.
 */
describe('where a thread opens', () => {
  it('routes to Messages', () => {
    expect(routeForNodeType('syn_thread')).toBe('messages');
  });

  /**
   * `ThingsApp::openInOwner` does `router.push({ name: route })`, so a value
   * that is not a route sends somebody nowhere. Only this entry is asserted:
   * `project`, `person`, `finance_month` and `pdf_highlight` name handlers
   * rather than routes and have never worked there, which is worth fixing on
   * its own rather than being pinned in place here.
   */
  it('names a route that exists, so Things can push to it', () => {
    const names = router.getRoutes().map((r) => r.name).filter(Boolean);
    expect(names).toContain(routeForNodeType('syn_thread'));
  });
});
