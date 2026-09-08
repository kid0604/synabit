import { describe, it, expect } from 'vitest';
import bar from '../AskBar.vue?raw';
import app from '../../../App.vue?raw';

/**
 * The screen is read before the bar is shown, and this is what keeps it so.
 *
 * Showing the bar moves the caret into its textarea, and focusing an input
 * collapses the document selection. So a bar that read `window.getSelection()`
 * for itself would read an empty one every single time — while passing any
 * test that never actually focused anything, and while looking completely
 * correct in review.
 *
 * That is the entire failure mode of this feature, it is invisible, and the
 * only place it can be caught cheaply is here: the capture belongs to the
 * keydown handler, and the bar is handed the result.
 */
describe('when what is on screen gets read', () => {
  it('is captured by the handler that opens the bar', () => {
    const opener = app.split('const openAskBar = () => {')[1]?.split('};')[0] ?? '';

    expect(opener, 'App.vue should still have openAskBar').toBeTruthy();
    expect(opener, 'the capture happens as the bar opens').toContain('captureFocus(');
  });

  it('happens before the bar is shown, not after', () => {
    const opener = app.split('const openAskBar = () => {')[1]?.split('};')[0] ?? '';

    expect(
      opener.indexOf('captureFocus('),
      'the selection has to be read before anything takes focus',
    ).toBeLessThan(opener.indexOf('askBarOpen.value = true'));
  });

  /**
   * The bar is handed what was read. If it ever reaches for the selection
   * itself, it is reaching after the caret has already moved into it.
   */
  it('is never read by the bar itself', () => {
    expect(bar).not.toContain('getSelection');
    expect(bar).not.toContain('captureFocus');
    expect(bar, 'the bar takes what it is given').toContain('focus?: SynFocus');
  });

  /** What was read is what is sent. A prop that never reaches the request is
   *  a feature that is wired up to nothing. */
  it('is what gets sent with the question', () => {
    expect(bar).toContain('focus: props.focus');
  });
});

/**
 * A lock screen is a promise that what is behind it stays behind it.
 *
 * Syn reads the whole vault, so a bar summoned over the lock would answer
 * questions about protected notes to whoever pressed the key. The first
 * version of this feature did exactly that: the bar rendered on `vaultPath`
 * alone, and Cmd+J worked while the PIN screen was up.
 */
describe('asking while the app is locked', () => {
  const guard = app.split('const askBarAllowed = computed(() => {')[1]?.split('});')[0] ?? '';

  it('has a guard to read', () => {
    expect(guard, 'App.vue should still have askBarAllowed').toBeTruthy();
  });

  it('refuses when the whole app is locked', () => {
    expect(guard).toContain('isAppLocked');
  });

  /** A mini-app the user protected on its own must not be reachable by
   *  keyboard shortcut either. */
  it('refuses when this mini-app is locked on its own', () => {
    expect(guard).toContain('isMiniAppAccessible');
  });

  it('gates both the shortcut and the component', () => {
    const opener = app.split('const openAskBar = () => {')[1]?.split('};')[0] ?? '';
    expect(opener, 'the shortcut checks it').toContain('askBarAllowed');
    expect(app, 'and the component is not even rendered').toContain('v-if="askBarAllowed"');
  });

  /** Locking while the bar is open is the same hole from the other side. */
  it('closes a bar that is already open when the app locks', () => {
    expect(app).toContain('watch(askBarAllowed');
  });
});

/**
 * The way out of the bar has to lead somewhere else.
 *
 * The bar saves its exchange as a real conversation and offers "continue in
 * Messages" so it is never a dead end. Asked *from* Messages, that link leads
 * to the room the user is standing in — which reads as broken rather than as
 * helpful. The first screenshot of this bar working showed exactly that.
 */
describe('continuing the exchange somewhere else', () => {
  const guard = bar.split('const canContinueElsewhere = computed(')[1]?.split(');')[0] ?? '';

  it('has a guard to read', () => {
    expect(guard, 'AskBar should still have canContinueElsewhere').toBeTruthy();
  });

  it('is offered only once there is an answer to continue', () => {
    expect(guard).toContain('conversationId');
    expect(guard).toContain('answer');
    expect(guard).toContain('!busy');
  });

  it('is not offered while the user is already in Messages', () => {
    expect(guard).toContain("props.focus?.app !== 'messages'");
  });
});

/**
 * Pressing Start twice with one name must not leave two of the same thread.
 *
 * It did. A vault three days into this feature held `General.md`,
 * `General (1).md` and `General (2).md`, created within five seconds of each
 * other, only the third holding anything — and the picker showed three
 * identical rows with no way to tell from there which was which. A name is how
 * a person identifies a piece of work, so a name already taken means they are
 * pointing at that work rather than asking for a second one.
 */
describe('starting a thread whose name is taken', () => {
  const start = bar.split('const startAndChoose = async () => {')[1]?.split('\n};')[0] ?? '';

  it('has the start handler to read', () => {
    expect(start, 'AskBar should still have startAndChoose').toBeTruthy();
  });

  it('looks for one by that name before creating', () => {
    expect(start).toContain('choices.value.find');
    expect(
      start.indexOf('choices.value.find'),
      'the lookup has to come before the create',
    ).toBeLessThan(start.indexOf('await startThread('));
  });

  it('switches to the existing one instead of making a twin', () => {
    const branch = start.split('if (existing) {')[1]?.split('}')[0] ?? '';
    expect(branch, 'it selects the one that is already there').toContain('choose(existing.id)');
    expect(branch, 'and does not go on to create').not.toContain('startThread');
  });

  /** Only open ones. A name reused long after that work closed is new work. */
  it('matches only threads that are still open', () => {
    expect(start).toContain('choices.value');
    expect(start).not.toContain('threads.value.find');
  });
});

/**
 * Creating a thread has to be visible in the place you created it.
 *
 * It was not. Start selected the new thread and closed the picker, so the only
 * evidence anything had happened was eleven grey pixels in the corner changing
 * from "No thread" to a name — invisible to somebody looking at the field they
 * had just typed into. They pressed Start twice more. The vault ended up with
 * `General.md`, `General (1).md` and `General (2).md`.
 *
 * Choosing still closes, because choosing is a decision that is finished.
 * Creating does not, because the thing worth seeing is the thread that now
 * exists.
 */
describe('seeing that a thread was created', () => {
  const start = bar.split('const startAndChoose = async () => {')[1]?.split('\n};')[0] ?? '';
  const created = start.split('const id = await startThread(title);')[1] ?? '';

  it('has the create branch to read', () => {
    expect(created, 'startAndChoose should still create').toBeTruthy();
  });

  it('selects the new thread without closing the picker', () => {
    expect(created, 'it becomes the current thread').toContain("emit('thread', id)");
    expect(created, 'and the list stays up to show it').not.toContain('pickingThread.value = false');
  });

  it('marks the row so the list says which one is new', () => {
    expect(created).toContain('justStarted.value = id');
    expect(bar, 'the row renders the mark').toContain("thread.id === justStarted");
  });

  /** Choosing an existing one is a finished decision, so it still closes. */
  it('still closes when an existing thread is chosen', () => {
    const choose = bar.split('const choose = (id: string | undefined) => {')[1]?.split('};')[0] ?? '';
    expect(choose).toContain('pickingThread.value = false');
  });

  /** The mark belongs to one opening of the picker, not forever. */
  it('forgets the mark when the picker is opened again', () => {
    const toggle = bar.split('const togglePicker = async () => {')[1]?.split('\n};')[0] ?? '';
    expect(toggle).toContain('justStarted.value = null');
  });
});
