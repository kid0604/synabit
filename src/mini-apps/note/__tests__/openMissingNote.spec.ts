import { describe, it, expect } from 'vitest';
import source from '../NoteApp.vue?raw';

/**
 * Opening something that is not there.
 *
 * `openNoteById` used to fall through to the editor for an id nothing in the
 * vault had: the redirect it does first only fires when the node **exists** and
 * belongs to another app. So a deleted note reached `loadNoteFile`, which does
 * not fail on a missing file — it waits. The reader got a spinner that never
 * stopped.
 *
 * Found through the newest route into it — the button that keeps a diagram as a
 * note, pressed after the note had been deleted — but every other route had it
 * too: a Syn source chip, a `[[wikilink]]`, a reminder, a row in Nexus.
 */
describe('opening a note that is gone', () => {
  const body = source.slice(
    source.indexOf('const openNoteById'),
    source.indexOf('defineExpose({ openNoteById'),
  );

  it('is a real function and this test is looking at it', () => {
    expect(body).toContain('resolveNoteId');
    expect(body).toContain('loadNoteFile');
  });

  it('stops before the editor when nothing in the vault has that id', () => {
    const guard = body.indexOf('if (!node)');
    const opens = body.indexOf("manager.viewMode.value = 'editor'");

    expect(guard, 'the missing-node guard is there').toBeGreaterThan(-1);
    expect(guard, 'and it comes before the editor is opened').toBeLessThan(opens);
    expect(body.slice(guard, opens)).toContain('return;');
  });

  /** The list is the one state this app can always show honestly, and it is
   *  where somebody who has just deleted a note expects to be. */
  it('shows the list rather than an editor with nothing in it', () => {
    expect(body).toContain("manager.viewMode.value = 'manager'");
  });

  /**
   * The existing redirect must keep working: something that is not a note goes
   * to the app that owns it, and this app is left as it was.
   */
  it('still hands a task or an event to the app that owns it', () => {
    expect(body).toContain('routeForNode(node.node_type, id)');
    expect(body).toContain("emit('open-node', id, route)");
  });
});
