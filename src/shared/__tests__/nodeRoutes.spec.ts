import { describe, it, expect } from 'vitest';
import {
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
