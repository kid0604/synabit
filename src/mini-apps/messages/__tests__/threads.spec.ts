import { describe, it, expect } from 'vitest';

import { grouped, closed, subtitle, GROUPS, byLastMoved } from '../threads';
import type { Thread, ThreadState } from '../../../shared/syn/useThreads';
import source from '../components/ChatSidebar.vue?raw';
import app from '../MessagesApp.vue?raw';
import panel from '../components/ThreadPanel.vue?raw';
import streamingIndicator from '../components/StreamingIndicator.vue?raw';
import chat from '../composables/useSynChat.ts?raw';
import bar from '../../../shared/syn/AskBar.vue?raw';
import types from '../types.ts?raw';
import bubble from '../components/MessageBubble.vue?raw';
import footingMark from '../components/FootingMark.vue?raw';
import settingsPanel from '../components/SynSettings.vue?raw';
import instructionsPanel from '../components/InstructionsPanel.vue?raw';
import enabledComposable from '../../../shared/syn/useSynEnabled.ts?raw';
import app_root from '../../../App.vue?raw';
import notificationCard from '../components/NotificationCard.vue?raw';
import synSettingsComposable from '../composables/useSynSettings.ts?raw';
import inspector from '../components/RunInspector.vue?raw';
import consent from '../composables/useSynConsent.ts?raw';
import { askPhrase, capabilityLabel } from '../composables/useSynConsent';
import choiceComposable from '../composables/useSynChoice.ts?raw';
import choiceCard from '../components/ChoiceCard.vue?raw';
import consentCard from '../components/ConsentCard.vue?raw';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

const thread = (over: Partial<Thread> & { id: string }): Thread => ({
  title: over.id,
  body: '',
  state: 'resting',
  waiting_for: null,
  opened: '2026-09-01T00:00:00Z',
  last_moved: '2026-09-01T00:00:00Z',
  ...over,
});

/**
 * The order this screen puts work in.
 *
 * Things already lists threads, because they are ordinary nodes, and it lists
 * them as rows sorted by a column. That is the right screen for "what is in my
 * vault". This one answers the question a table cannot — *whose move is it* —
 * and the grouping is the whole of that answer, so it is checked here rather
 * than by looking at a screenshot.
 */
describe('arranging what is open', () => {
  it('puts what Syn owes before what the user owes', () => {
    expect(GROUPS.indexOf('mine')).toBeLessThan(GROUPS.indexOf('yours'));
  });

  it('puts both moves before the ones nobody has to make', () => {
    for (const nobody of ['world', 'resting'] as ThreadState[]) {
      expect(GROUPS.indexOf('yours'), nobody).toBeLessThan(GROUPS.indexOf(nobody));
    }
  });

  /**
   * Finished work belongs in Nexus and Things. A screen showing everything
   * that ever happened stops being a screen about now.
   */
  it('does not group closed work with open work', () => {
    expect(GROUPS).not.toContain('closed');

    const threads = [thread({ id: 'a', state: 'closed' }), thread({ id: 'b', state: 'mine' })];
    expect(grouped(threads).flatMap((g) => g.threads.map((t) => t.id))).toEqual(['b']);
    expect(closed(threads).map((t) => t.id)).toEqual(['a']);
  });

  /** Four headings over one thread reads as three things missing. */
  it('leaves out a group with nothing in it', () => {
    const groups = grouped([thread({ id: 'a', state: 'mine' })]);
    expect(groups).toHaveLength(1);
    expect(groups[0].state).toBe('mine');
  });

  it('orders each group by when it last moved', () => {
    const groups = grouped([
      thread({ id: 'old', state: 'yours', last_moved: '2026-09-01T00:00:00Z' }),
      thread({ id: 'new', state: 'yours', last_moved: '2026-09-06T00:00:00Z' }),
      thread({ id: 'mid', state: 'yours', last_moved: '2026-09-03T00:00:00Z' }),
    ]);
    expect(groups[0].threads.map((t) => t.id)).toEqual(['new', 'mid', 'old']);
  });

  it('sorts newest first', () => {
    const a = thread({ id: 'a', last_moved: '2026-09-06T00:00:00Z' });
    const b = thread({ id: 'b', last_moved: '2026-09-01T00:00:00Z' });
    expect(byLastMoved(a, b)).toBeLessThan(0);
  });
});

/**
 * The line under a thread's name is what turns a list of names into a list of
 * work.
 */
describe('what a row says under the name', () => {
  it('says what it is waiting for, when somebody wrote that down', () => {
    expect(subtitle(thread({ id: 'a', waiting_for: 'quyết per-seat hay flat' }))).toBe(
      'quyết per-seat hay flat',
    );
  });

  it('falls back to the first real line of the body', () => {
    const body = '## What this is\n\nChốt giá cho bản Pro\n\n## What we found\n';
    expect(subtitle(thread({ id: 'a', body }))).toBe('Chốt giá cho bản Pro');
  });

  /**
   * A thread created a minute ago holds only the headings it was made with. It
   * has nothing to say yet, and saying `## What this is` would be worse than
   * saying nothing.
   */
  it('says nothing when the thread is still only its headings', () => {
    expect(subtitle(thread({ id: 'a', body: '## What this is\n\n## Still open\n' }))).toBe('');
    expect(subtitle(thread({ id: 'a', body: '' }))).toBe('');
  });

  it('prefers what it is waiting for over the body', () => {
    expect(
      subtitle(thread({ id: 'a', waiting_for: 'Minh trả lời', body: 'Chốt giá cho bản Pro' })),
    ).toBe('Minh trả lời');
  });

  /** An empty string in the file is not something somebody wrote down. */
  it('ignores a blank waiting_for', () => {
    expect(subtitle(thread({ id: 'a', waiting_for: '   ', body: 'thân bài' }))).toBe('thân bài');
  });
});

/**
 * Closing is a state, and the folded section is what makes that safe.
 *
 * `closed()` sat exported and tested with nothing calling it for a while, after
 * the threads screen it was written for was folded back into Messages. An
 * export nobody calls is a claim the code cannot keep — so either it goes or it
 * gets used, and using it answers a real question: closing a thread is one
 * click, and without somewhere to see finished work, undoing that click meant
 * going to Things to find the file.
 */
describe('finished work', () => {
  const sidebar = source;

  it('is listed from the same reading the open work comes from', () => {
    expect(sidebar).toContain("import { grouped, closed, subtitle } from '../threads'");
    expect(sidebar, 'and it is actually called').toContain('closed(props.threads');
  });

  it('is folded rather than shown', () => {
    expect(sidebar).toContain('showClosed');
  });

  it('can be picked up again', () => {
    expect(sidebar).toContain("emit('reopenThread'");
  });
});

/**
 * Closing and deleting are different things, and both have to be offered.
 *
 * They were not. The sidebar had one button — the tick — so a vault three
 * threads deep in accidents got all three filed as *finished work*: two empty
 * files and one holding two live questions, all marked closed within twenty
 * seconds. People press the button that is there.
 *
 * **Closed** means the work finished; the thread is kept and folded away.
 * **Deleted** means it should not have existed; it goes to the trash, which is
 * recoverable like every other node this app removes.
 */
describe('finishing a thread versus removing one', () => {
  const sidebar = source;

  it('offers both, and they are not the same action', () => {
    expect(sidebar).toContain("emit('closeThread'");
    expect(sidebar).toContain("emit('deleteThread'");
  });

  it('says which is which where somebody is deciding', () => {
    expect(en.syn, 'en').toHaveProperty('close_thread');
    expect(en.syn, 'en').toHaveProperty('delete_thread');
    expect(vi.syn, 'vi').toHaveProperty('close_thread');
    expect(vi.syn, 'vi').toHaveProperty('delete_thread');
    // The delete label carries the distinction, because a bare "Delete" beside
    // a tick reads as the same idea twice.
    expect(en.syn.delete_thread.length).toBeGreaterThan('Delete'.length);
  });

  /** A thread that was deleted by mistake comes back. */
  it('trashes rather than unlinks', () => {
    expect(app).toContain('ns.trashNode({ relPath: id })');
    expect(app, 'never the outright delete').not.toContain('deleteNode(');
  });

  /**
   * Renaming changes the title, not the file. A thread's id *is* its path and
   * it travels — into `Focus.thread`, into the ask bar, across the next
   * question — and `thread::get` answers `None` quietly, so moving the file
   * would show up as Syn losing the thread without saying so.
   */
  it('renames the title and leaves the path alone', () => {
    expect(app).toContain('renameThread');
    expect(app, 'through the ordinary node write').toContain("nodeType: 'syn_thread'");
    expect(app, 'and never by moving the file').not.toContain('renameNode(');
  });
});

/**
 * The number that says whether threads are worth keeping is on screen.
 *
 * Collecting it is not enough. `recall` went uncalled across fifteen runs and
 * the skill detector fired on none of seventeen; both were invisible until
 * somebody ran a test by hand, and neither would have been noticed by using the
 * app. A line reading "0 of 12 runs wrote back" is the earliest warning that
 * threads are going the same way, and it only works if somebody sees it.
 */
describe('whether threads are used', () => {
  it('is shown in the sidebar, not only counted', () => {
    expect(source, 'the aggregate line').toContain("t('threads.stats'");
    expect(source).toContain('runs_that_wrote_back');
  });

  it('warns when a thread has been read and never written to', () => {
    expect(source, 'the aggregate turns amber at zero').toContain('amber');
    expect(panel, 'and so does one thread on its own').toContain('amber');
  });

  it('says both halves in both languages', () => {
    for (const [lang, locale] of [['en', en], ['vi', vi]] as const) {
      for (const key of ['stats', 'usage']) {
        expect(locale.threads, `${lang}.threads.${key}`).toHaveProperty(key);
      }
    }
    // The pair is the point: asks alone is the easy number, and writes alone
    // has no denominator.
    for (const locale of [en, vi]) {
      expect(locale.threads.stats).toContain('{wrote}');
      expect(locale.threads.usage).toContain('{runs}');
      expect(locale.threads.usage).toContain('{wrote}');
    }
  });
});

/**
 * No data reads as nothing, never as zero.
 *
 * `Run.thread` was added after twenty-five runs already sat on disk, so the
 * count on the day it shipped was `0 of 25` — and that zero says nothing about
 * threads, only that the field is new. A line reading "0 of 25 wrote back" is
 * exactly the plausible-looking wrong number this repository keeps catching
 * itself on, and it would be read as a verdict.
 *
 * So both places say nothing until there is something to say. Amber is for a
 * thread that *was* in front of Syn and got nothing back — a real finding —
 * not for an empty denominator.
 */
describe('the count before there is anything to count', () => {
  it('says nothing at all when no run has had a thread', () => {
    expect(source, 'the aggregate is hidden on an empty denominator')
      .toContain('v-if="stats && stats.runs_in_a_thread"');
  });

  it('says nothing about a thread nothing has run in', () => {
    expect(panel, 'no usage, no line').toContain('v-if="usage"');
  });

  /** Amber means "asked and got nothing back", not "never asked". */
  it('only warns once there is a denominator', () => {
    expect(panel).toContain('usage.runs && !usage.wrote_back');
    expect(source).toContain('stats.runs_that_wrote_back ?');
  });
});

/**
 * A fast answer must not wear a spinner.
 *
 * The same three dots for a count answered from the index and for a question
 * that will take four rounds is what makes the fast one feel slow and the slow
 * one feel broken. The backend decides the tempo before the work starts — see
 * `syn::tempo` — and this is the half that puts it on screen, which is the half
 * the design cared about: *"im lặng bốn phút là cách nhanh nhất để mất tin"*.
 */
describe('saying how heavy a turn is', () => {
  const indicator = streamingIndicator;

  it('is listened for, not inferred', () => {
    expect(chat, 'the composable subscribes').toContain("'syn-tempo'");
    expect(bar, 'and so does the bar').toContain("'syn-tempo'");
  });

  it('is forgotten at the start of each turn', () => {
    expect(chat).toContain('tempo.value = null');
    expect(bar).toContain('tempo.value = null');
  });

  /** An instant turn says so instead of showing work that is not happening. */
  it('shows something other than the thinking dots when it is instant', () => {
    expect(indicator).toContain("tempo === 'instant'");
    expect(bar).toContain("tempo === 'instant'");
  });

  /** And a working turn says it may take a moment — the sentence that lets
   *  somebody look away. */
  it('says a working turn may take a moment', () => {
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('tempo_instant');
      expect(locale.syn).toHaveProperty('tempo_working');
    }
    expect(indicator).toContain('tempo_working');
  });
});

/**
 * Nothing disappears from this screen without a question first.
 *
 * Both delete buttons live inside a row, appear on hover, and sit a few pixels
 * from the row itself — so the click that removes a month of conversation looks
 * exactly like the click that opens it. Neither asked anything, and the i18n
 * file still carried a `delete_conversation_title` key that nothing rendered,
 * which is what a lost confirmation looks like six months later.
 *
 * These check the gate rather than the styling: that the emit lands on a
 * question, that the question names the object, and that the two answers to
 * *where does it go* are both told to the person deciding — because a warning
 * that overstates the damage is how people learn to click through warnings.
 */
describe('asking before removing', () => {
  it('sends both delete buttons to a question, not to the deletion', () => {
    expect(app, 'threads').toContain('@delete-thread="askDeleteThread"');
    expect(app, 'conversations').toContain('@delete-conversation="askDeleteConversation"');
    expect(app, 'and the dialog is on screen').toContain('<ConfirmModal');
    expect(app).toContain('is-destructive');
  });

  /**
   * The names matter: the two functions that actually remove something are
   * reachable only from `confirmDelete`, so a future edit that wires a button
   * straight to one of them is visible in the diff rather than silent.
   */
  it('keeps the removal itself behind the answer', () => {
    expect(app).toContain('const confirmDelete');
    expect(app).toContain('reallyDeleteConversation');
    expect(app).toContain('reallyDeleteThread');
    expect(app, 'nothing calls the removal from the template').not.toContain(
      '@delete-thread="reallyDeleteThread"'
    );
  });

  /** Escape answers "no", like it does for every other dialog. */
  it('lets Escape dismiss the question before it stops a stream', () => {
    expect(app).toContain("if (e.key === 'Escape' && pendingDelete.value)");
  });

  /**
   * The two deletions are not equally severe and the copy says so. A
   * conversation is `remove_file` — no trash, no history. A thread is
   * `trash_node_file`, like every other node in the vault.
   */
  it('tells the truth about where each one goes, in both languages', () => {
    for (const [name, locale] of [['en', en], ['vi', vi]] as const) {
      expect(locale.syn, name).toHaveProperty('delete_thread_title');
      expect(locale.syn, name).toHaveProperty('delete_thread_body');
      expect(locale.syn, name).toHaveProperty('delete_conversation_title');
      expect(locale.syn, name).toHaveProperty('delete_conversation_body');
      // Both name the thing being removed, so the dialog is never about
      // "this item".
      expect(locale.syn.delete_thread_body, name).toContain('{title}');
      expect(locale.syn.delete_conversation_body, name).toContain('{title}');
    }
    expect(en.syn.delete_thread_body, 'a thread is recoverable').toContain('Trash');
    expect(vi.syn.delete_thread_body, 'a thread is recoverable').toContain('Thùng rác');
    expect(en.syn.delete_conversation_body, 'a conversation is not').toContain('for good');
    expect(vi.syn.delete_conversation_body, 'a conversation is not').toContain('xoá hẳn');
  });
});

/**
 * Saying what an answer stood on, as a state rather than a hedge.
 *
 * The state itself is decided in Rust, by arithmetic on the run's transcript —
 * `syn::footing`, and its tests are the ones that check the decision. What is
 * checked here is the half that makes it worth deciding: that it reaches the
 * screen, that it survives reopening the conversation, and that the three
 * states are told apart in words rather than by a number nobody can audit.
 */
describe('what an answer is standing on', () => {
  it('rides on the message and not only on a live event', () => {
    expect(types, 'the type exists').toContain("export type Footing = 'grounded' | 'inferred' | 'guessing'");
    expect(types, 'and hangs off the message, so it survives a reload').toContain('footing?: Footing;');
    expect(bubble, 'and the bubble draws it').toContain('<FootingMark');
  });

  /** Only the state that should change what the reader does next is coloured.
   *  Colouring two of three is how a warning becomes wallpaper. */
  it('colours the guess and leaves the other two grey', () => {
    expect(footingMark).toContain('text-amber-600');
    expect(footingMark.match(/text-amber/g) ?? [], 'exactly one coloured state').toHaveLength(2);
  });

  it('names all three in both languages, with the longer reason behind them', () => {
    for (const [name, locale] of [['en', en], ['vi', vi]] as const) {
      for (const state of ['grounded', 'inferred', 'guessing']) {
        expect(locale.syn, `${name}.${state}`).toHaveProperty(`footing_${state}`);
        expect(locale.syn, `${name}.${state} reason`).toHaveProperty(`footing_${state}_why`);
      }
    }
  });

  /** No `confidence: 0.73`. A number nobody can check is read as a probability
   *  by everybody who sees one. */
  it('is never a number', () => {
    expect(footingMark).not.toMatch(/confidence|\bscore\b|%/i);
    for (const locale of [en, vi]) {
      for (const state of ['grounded', 'inferred', 'guessing']) {
        expect((locale.syn as Record<string, string>)[`footing_${state}`]).not.toMatch(/\d/);
      }
    }
  });
});

/**
 * The switch, and what it is a test of.
 *
 * Syn's surface grew — threads inside this app, an ask bar over every other one
 * — and there was no off position. The design put it plainly: if turning Syn
 * off leaves Synabit unusable, then Syn has taken over more of the app than it
 * should have, and the switch is how you find that out.
 *
 * So these check both halves: that off really is off, and that it stops at
 * Syn's edge.
 */
describe('turning Syn off', () => {
  it('is enforced by the backend and only reflected by the screen', () => {
    expect(chat, 'the refusal is recognised, not guessed at').toContain('SWITCHED_OFF');
    expect(
      enabledComposable,
      'and the composable says out loud that it is not the enforcement'
    ).toContain('not to be the thing that refuses');
  });

  it('closes the ask bar everywhere, not just in this app', () => {
    expect(app_root).toContain('useSynEnabled');
    expect(app_root).toContain('if (!synEnabled.value) return false;');
  });

  /** An unreadable settings file must not read as "off": the backend would
   *  still be answering, and the app would disagree with itself. */
  it('assumes Syn is on when it cannot tell', () => {
    expect(enabledComposable).toContain('settings?.enabled ?? true');
    expect(enabledComposable).toContain('enabled.value = true;');
  });

  it('says why the composer is gone instead of showing nothing', () => {
    expect(app, 'the off state is drawn').toContain('v-if="!synEnabled"');
    expect(app, 'and offers the way back').toContain('syn_off_turn_on');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('syn_off_title');
      expect(locale.syn).toHaveProperty('syn_off_body');
      expect(locale.syn).toHaveProperty('enabled_hint');
    }
  });

  /**
   * The line the whole switch is a test of. Turning off the colleague must not
   * turn off the calendar: `chat_engine` sends "this task is overdue" as
   * *Synabit System*, and those are the app doing its job.
   */
  it('leaves what is not Syn alone, and says so where somebody will read it', () => {
    expect(en.syn.enabled_hint).toContain('reminders');
    expect(vi.syn.enabled_hint).toContain('Nhắc việc');
  });
});

/**
 * `SYN.md` grown into a contract with four questions in it.
 *
 * The file existed already; what it lacked was any reason to open it. A blank
 * page captioned "tell Syn how to work with you" is a page nobody fills in —
 * not for want of opinions, but because the sentences that actually change an
 * answer are not the ones that come to mind first.
 */
describe('the standing instructions as a contract', () => {
  it('offers the draft from Rust rather than a second copy of it', () => {
    expect(instructionsPanel).toContain("invoke<string>('syn_instructions_template')");
    expect(instructionsPanel, 'two copies of a contract drift').not.toContain(
      '## When I am not sure'
    );
  });

  it('offers it only when there is nothing there to overwrite', () => {
    expect(instructionsPanel).toContain('v-if="!body.trim()"');
    expect(instructionsPanel).toContain('@click="useTemplate"');
  });

  it('fills the box and not the vault', () => {
    expect(instructionsPanel).toContain('body.value = await invoke');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('instructions_use_template');
      expect(locale.syn).toHaveProperty('instructions_template_hint');
    }
  });

  /**
   * The sentence `instructions.rs` opens by condemning: *the thing that shapes
   * every answer Syn ever gives was reachable only through a textarea in a
   * settings modal.* Moving the storage into a file fixed where the words are
   * kept and left that sentence true about how anybody reaches them.
   */
  it('is a screen of its own and not a field in the settings modal', () => {
    expect(source, 'the sidebar lists it').toContain("{ kind: 'instructions' }");
    expect(app, 'and the app draws a pane for it').toContain('<InstructionsPanel');
    expect(
      settingsPanel,
      'and settings no longer edits it — two places is how they disagree'
    ).not.toContain('syn_save_instructions');
  });

  /** It says how much of it Syn actually gets. A silently truncated
   *  instruction is one somebody believes is in force and is not. */
  it('says how much of it reaches Syn', () => {
    expect(instructionsPanel).toContain('instructions_over_budget');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('instructions_budget');
      expect(locale.syn).toHaveProperty('instructions_where');
    }
  });
});

/**
 * The three-voice picker, gone.
 *
 * It chose one of three: two hard-coded Vietnamese and a pronoun pair on the
 * user's behalf, and the third was the rule that makes a bilingual app work —
 * which is not a personality and now rides unconditionally. What replaces it is
 * a file where somebody can say anything rather than pick one of three things.
 */
describe('retiring the personality setting', () => {
  it('is gone from the settings screen and from the settings type', () => {
    expect(settingsPanel).not.toContain('personalityOptions');
    expect(settingsPanel).not.toContain('settings.personality');
    expect(synSettingsComposable, 'and from what Reset restores').not.toContain(
      "personality: 'auto'"
    );
  });

  it('takes its strings with it', () => {
    for (const locale of [en, vi]) {
      expect(locale.syn).not.toHaveProperty('personality_casual');
      expect(locale.syn).not.toHaveProperty('personality_professional');
      expect(locale.syn).not.toHaveProperty('settings_personality');
    }
  });

  /** A section the frontend can draw and Rust can never send is dead code that
   *  reads as a feature. */
  it('is no longer a prompt section the panel can draw', () => {
    expect(types).not.toContain("| 'personality'");
  });

  /** The settings screen says where it went, rather than leaving somebody to
   *  wonder whether Syn forgot how they wanted to be spoken to. */
  it('says where it went', () => {
    expect(settingsPanel).toContain('instructions_moved');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('instructions_moved');
    }
  });
});

/**
 * Things Syn noticed, told apart from things the calendar is announcing.
 *
 * The detectors are arithmetic and live in `syn::notice`, where their own tests
 * check the decisions. What matters on this side is that the two kinds of card
 * are not the same card: a reminder is time-bound and the app owes it to you; a
 * notice is a colleague saying they spotted something and *did nothing about
 * it*. Rendering them identically would make the second read as the first.
 */
describe('saying what was noticed', () => {
  it('draws each kind with its own icon', () => {
    for (const subtype of ['syn_stuck_thread', 'syn_contradiction', 'syn_degrading_skill']) {
      expect(notificationCard, subtype).toContain(subtype);
    }
  });

  it('tells a notice apart from a reminder, and tints it differently', () => {
    expect(notificationCard).toContain("startsWith('syn_')");
    expect(notificationCard, 'notices are Syn-coloured').toContain('text-violet-500');
    expect(notificationCard, 'reminders stay as they were').toContain('text-blue-500');
  });

  /**
   * The card already names the sender, and the backend writes "Syn" rather
   * than "Synabit System" on these. That name is the whole difference between
   * the calendar doing its job and a colleague speaking.
   */
  it('shows who said it, so the two voices are not one voice', () => {
    expect(notificationCard).toContain('notification.sender?.name');
  });
});

/**
 * What Syn can reach, as a screen.
 *
 * The inspector could say what Syn *did* (the transcript), what it was *told*
 * (the prompt), and what it had been *allowed* (the ledger) — and had no answer
 * at all for what it can reach in the first place. That is the question people
 * ask before deciding to trust something, not after; and the catalogue existed
 * from the first day, read in exactly one place in the whole codebase, to
 * validate recipe step names.
 */
describe('what Syn can reach', () => {
  it('is its own tab, not a block on the permissions screen', () => {
    expect(inspector).toContain("type Tab = 'runs' | 'prompt' | 'tools'");
    expect(inspector).toContain("'runs', 'prompt', 'tools', 'memory', 'skills', 'permissions'");
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('inspector_tab_tools');
    }
  });

  it('reads the catalogue from Rust rather than listing tools in the frontend', () => {
    expect(inspector).toContain("invoke<ToolCard[]>('syn_list_tools')");
    expect(inspector, 'no second copy of the tool names').not.toContain("'query_nodes'");
  });

  /**
   * The shape of the list *is* the answer. Fourteen that only read, nine that
   * change one note, four that change many files at once — a flat alphabetical
   * list of twenty-seven hides exactly that.
   */
  it('groups them by what they need, most powerful last', () => {
    expect(inspector).toContain('toolGroups');
    expect(inspector).toContain(
      "CAPABILITY_ORDER = ['VaultRead', 'VaultWrite', 'VaultStructural']"
    );
  });

  /** A tool the registry cannot classify is a bug, and burying it under the
   *  classified ones is how a bug becomes permanent. */
  it('puts anything unclassified first, in amber', () => {
    expect(inspector).toContain('tools_unclassified');
    expect(inspector).toContain('// Unclassified first: it is a bug and should not be buried.');
  });

  /** Derived from the capability in Rust rather than declared twice, so the
   *  two can never disagree. */
  it('says what puts each one back', () => {
    expect(inspector).toContain("tool.reversal?.kind === 'nothing'");
    expect(inspector).toContain("tool.reversal?.kind === 'irreversible'");
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('tools_undo');
      expect(locale.syn).toHaveProperty('tools_undo_nothing');
    }
  });

  /**
   * The descriptions are the model's, word for word, and the screen says so.
   * A kinder paraphrase written for this panel would be a second wording to
   * keep in step — and the one people read would be the one never sent.
   */
  it('shows the model’s own words and admits that is what they are', () => {
    expect(inspector).toContain('tools_verbatim');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('tools_verbatim');
    }
  });

  /** A catalogue label is not a consent question. Twenty-five of the latter
   *  would read as twenty-five pending questions. */
  it('labels a capability rather than asking about it', () => {
    expect(consent).toContain('export const capabilityLabel');
    expect(consent).toContain("key.replace('syn.consent_', 'syn.cap_')");
    expect(inspector).toContain('capabilityLabel');
  });
});

/**
 * What a turn actually costs, including the part that is not in the prompt.
 *
 * The tool declarations are the `tools` field of the request, not text in the
 * system prompt, so `PromptPlan` never saw them and neither did this panel.
 * Every figure on the screen was exactly right and the screen as a whole
 * understated a turn by 4,505 estimated tokens — more than the entire fixed
 * prompt costs. A number that is accurate and misleading is the failure this
 * codebase keeps finding.
 */
describe('what one turn costs', () => {
  it('counts the tool declarations, which are not in the prompt text', () => {
    expect(types, 'the preview carries them').toContain('tools: ToolPayload;');
    expect(inspector, 'and the panel draws them').toContain('preview.tools.count');
    expect(inspector).toContain('tools_payload');
  });

  it('adds the two halves, because neither is the answer alone', () => {
    expect(inspector).toContain('preview.value.est_tokens + preview.value.tools.est_tokens');
    expect(inspector).toContain('prompt_turn_total');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('prompt_turn_total');
    }
  });

  /**
   * Against a hosted model the totals are a cost; against Ollama's default
   * 8,192 they are a wall. The warning names the default a local install
   * actually gets rather than whatever this vault has been set to.
   */
  it('says when a turn has eaten a small local window', () => {
    expect(inspector).toContain('const SMALL_WINDOW = 8192');
    expect(inspector).toContain('overWindow');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('prompt_over_window');
    }
  });

  /** The declarations are held to a ceiling in Rust, and the bar shows how
   *  full it is — so the next tool added is visible rather than free. */
  it('shows the declarations against their own budget', () => {
    expect(inspector).toContain('toolsUsed');
    expect(inspector).toContain('preview.tools.budget_chars');
  });
});

/**
 * *Which one?* — asked by the engine, never by the model.
 *
 * The obvious shape is an `ask_user` tool, and it is the shape this codebase
 * has already watched fail: `recall` went uncalled across fifteen runs, and a
 * model confident enough to pick one of three notes is exactly a model that
 * does not feel it needs to ask. So the engine decides, from what it watched
 * happen — a query returned several, the next destructive call named one of
 * them — and the run stops there.
 */
describe('asking which one', () => {
  it('is a run state, not a tool the model has to remember', () => {
    expect(types).toContain("| 'awaiting_choice'");
    expect(inspector, 'and the panel can draw it').toContain('awaiting_choice');
    expect(choiceComposable).toContain("listen<ChoiceEvent>('syn-choice-needed'");
  });

  /**
   * Consent means a permission was never granted and something is refused.
   * This means the work is fine and Syn will not guess one detail. Two cards
   * that look alike teach one reflex for both.
   */
  it('does not look like the consent card, because it does not mean the same thing', () => {
    expect(choiceCard, 'not the consent amber').not.toContain('border-amber-300');
    expect(choiceCard).toContain('border-violet-200');
    expect(consentCard, 'and consent keeps its own colour').toContain('border-amber-300');
  });

  /** The model's pick is marked rather than hidden: it is usually right, and a
   *  question that makes somebody redo the search is one they stop answering. */
  it('shows which one Syn was about to act on', () => {
    expect(choiceCard).toContain('candidate.id === choice.chose');
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('choice_syn_picked');
      expect(locale.syn).toHaveProperty('choice_title');
    }
  });

  /** The question says *delete* or *change*, so it names what is about to
   *  happen rather than asking abstractly. */
  it('says what it was about to do', () => {
    expect(choiceCard).toContain("choice.tool === 'trash_node'");
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('choice_remove');
      expect(locale.syn).toHaveProperty('choice_change');
    }
  });

  /**
   * Answering records the decision; sending is still the person's move.
   *
   * Deliberately **not** what consent does any more. The two questions differ
   * in what the answer is: *may I?* is answered by the button and nothing else
   * is needed, so consent carries on. *Which one?* is a fact the next message
   * has to carry, and a person who picked from three is often about to say
   * something more — "that one, but only the heading". Sending for them takes
   * that sentence away.
   */
  it('puts the answer in the composer instead of carrying on by itself', () => {
    expect(choiceComposable).toContain("invoke('syn_answer_choice'");
    expect(app, 'the pick lands in the box').toContain('chatPanel.value?.prefill');
    expect(choiceComposable, 'and nothing here resumes a run').not.toContain('sendMessage');
    for (const locale of [en, vi]) {
      expect(locale.syn.choice_explainer.length).toBeGreaterThan(20);
    }
  });
});

/**
 * What an answer stands on, said in words that are still true.
 *
 * `Grounded` promised *from your vault* since the day it existed, and browsing
 * made that a lie: the label sat above a chip pointing at `warofsoccer.com`.
 * The mark is about whether there is a source to open, never about where the
 * source lives.
 */
describe('the footing labels', () => {
  it('does not claim the vault for something read off the web', () => {
    // The short label and the tally line stand alone with no room to qualify
    // anything, so neither may name a place at all.
    for (const locale of [en, vi]) {
      for (const key of ['footing_grounded', 'footing_tally'] as const) {
        expect(locale.syn[key].toLowerCase(), `${key} still says vault`).not.toMatch(/vault/);
      }
    }

    // The explanation has room, so it names both rather than neither — the
    // vault is still where most of them come from.
    for (const locale of [en, vi]) {
      const why = locale.syn.footing_grounded_why.toLowerCase();
      expect(why).toContain('vault');
      expect(why, 'and the other place a source can be').toMatch(/web|mạng/);
    }
  });

  /** `Inferred` is the one that really is about the vault — retrieval only ever
   *  looks there — so it keeps the word. */
  it('keeps the vault where the vault is the whole point', () => {
    expect(en.syn.footing_inferred_why.toLowerCase()).toContain('vault');
    expect(vi.syn.footing_inferred_why.toLowerCase()).toContain('vault');
  });

  /** Every mark still has a label and a reason, in both languages. */
  it('still explains all three, in both languages', () => {
    for (const locale of [en, vi]) {
      for (const mark of ['grounded', 'inferred', 'guessing']) {
        expect(locale.syn[`footing_${mark}` as keyof typeof locale.syn]).toBeTruthy();
        expect(locale.syn[`footing_${mark}_why` as keyof typeof locale.syn]).toBeTruthy();
      }
    }
  });
});

/**
 * The first tool that leaves the machine.
 *
 * The consent ledger has carried `NetRead` since P4 and nothing ever used it.
 * What is checked on this side is that the screen can say what it is: a
 * permission scoped to a host when one call is about to happen, and a plain
 * description of the tool when the catalogue is only asking what it does.
 */
describe('reading the web', () => {
  it('says what using the browser is, rather than “reads from .”', () => {
    // Asserted through the function rather than by finding a branch in the
    // source: the shape of the answer moved twice — a special case in
    // `capabilityLabel`, then a special case in `askPhrase`, then a capability
    // of its own — and a test pinned to where the code sat would have called
    // each of those a regression.
    expect(capabilityLabel('Browse').key).toBe('syn.cap_browse');
    expect(capabilityLabel({ NetRead: { domain: 'bbc.co.uk' } })).toEqual({
      key: 'syn.cap_netread',
      values: { domain: 'bbc.co.uk' },
    });
    for (const locale of [en, vi]) {
      expect(locale.syn).toHaveProperty('cap_browse');
      expect(locale.syn).toHaveProperty('cap_netread');
    }
  });

  /** The scoped sentence still names the host, because that is what was
   *  actually granted. */
  it('keeps the host in the sentence when there is one', () => {
    expect(en.syn.cap_netread).toContain('{domain}');
    expect(vi.syn.cap_netread).toContain('{domain}');
  });

  /**
   * And it asks about the browser, not about a website.
   *
   * `NetRead { domain }` suits *"may I read bbc.co.uk"*. `browse` is not that
   * act: given a question it searches, then opens whatever the results point
   * at — so the host is unknown when the card is drawn, and already read by the
   * time it is known. The card first showed that hole as *"Syn wants to read
   * from ."*, and the honest repair is not a better sentence about a host but a
   * different question.
   */
  it('asks whether Syn may use the browser, not to read one site', () => {
    const { key, values } = askPhrase('Browse');
    expect(key).toBe('syn.consent_browse');
    expect(values, 'nothing to interpolate, so nothing to leave blank').toEqual({});

    for (const locale of [en, vi]) {
      const sentence = locale.syn.consent_browse;
      expect(sentence).toBeTruthy();
      expect(sentence, 'no placeholder with nothing behind it').not.toContain('{');
      expect(sentence.toLowerCase(), 'and it names the browser').toMatch(
        /browser|trình duyệt/,
      );
    }
  });

  /**
   * "Just this once" meant *for the thing I have just asked for* to everybody
   * pressing it, and one outward call to the code. A question needing three
   * searches therefore stopped three times: eight cards for three questions.
   * The button now says what it always meant, and the explainer says how long
   * it lasts — because a permission whose length nobody can see is one people
   * grant without knowing what they granted.
   */
  it('says how long a yes lasts', () => {
    for (const locale of [en, vi]) {
      expect(locale.syn.consent_once.toLowerCase(), 'not "once"').not.toMatch(
        /once|lần này$/,
      );
      // The explainer has to quote the button it is explaining, or somebody
      // reads a rule about a control they cannot find.
      expect(locale.syn.consent_explainer).toContain(locale.syn.consent_once);
    }
    expect(en.syn.consent_explainer.toLowerCase()).toContain('next one asks again');
    expect(vi.syn.consent_explainer.toLowerCase()).toContain('câu sau sẽ hỏi lại');
  });
});

/**
 * A question, an answer, and then something happening.
 *
 * The card used to ask, take the answer, and stop — leaving the person to type
 * *go on*. Pressing **Just this once** is saying go on; being asked to then say
 * it again in words is being asked the same question twice, and what it looked
 * like from outside was silence.
 *
 * Worse than rude: `Once` writes nothing to the ledger by design, so an answer
 * that reached nothing meant the button granted nothing. The next attempt asked
 * the identical question. It was, until this, a control that did not work.
 */
describe('answering a consent card', () => {
  it('says whether there was a question, so the caller knows to carry on', () => {
    expect(consent).toContain("invoke<boolean>('syn_answer_consent'");
    expect(consent).toContain('Promise<boolean>');
  });

  it('carries on with the question already asked, not a new empty one', () => {
    expect(app).toContain('const onConsent');
    expect(app, 'the card is wired to it').toContain('@consent="onConsent"');
    expect(chat, 'the send path can say it is a continuation').toContain('resumeRun?: string');
    expect(types).toContain('resume_run?: string');
  });

  /**
   * And it carries on with *what was asked about*.
   *
   * The run's id goes back, not a flag, because the run is what holds the call
   * it was about to make. Without it the backend re-works the turn from the
   * user's message — which, in the transcript, followed permission to read one
   * particular site with a fresh DuckDuckGo search. The permission was granted
   * and spent on nothing, and the person was asked a third time.
   */
  it('sends back the run that stopped, so the granted call is the one that runs', () => {
    expect(app, 'the id is read before the card is cleared').toContain(
      'consentPending.value?.run_id',
    );
    expect(app, 'and it is what gets sent').toMatch(/sendMessage\([\s\S]{0,220}stopped,/);
    expect(app, 'nothing carries on without one').toContain('!wasAsked || !stopped');
  });

  /**
   * The stopped run leaves an assistant turn with no words in it. Left alone it
   * sits above the real answer as an empty bubble, and goes back to the model
   * as a turn that says nothing.
   */
  it('clears the empty turn the stopped run left behind', () => {
    expect(app).toContain("last?.role === 'assistant' && !last.content.trim()");
  });

  /**
   * A refusal carries on too. The tool comes back refused and Syn says so —
   * the alternative is a card that asks, accepts a no, and then says nothing,
   * which is indistinguishable from the bug this fixed.
   */
  it('promises an answer either way, in both languages', () => {
    for (const locale of [en, vi]) {
      expect(locale.syn.consent_explainer.length).toBeGreaterThan(20);
      expect(
        locale.syn.consent_explainer,
        'it no longer tells anybody to ask again by hand',
      ).not.toMatch(/tell Syn to carry on|bảo Syn làm tiếp/);
    }
  });
});

/**
 * A page Syn read is something you can go and check.
 *
 * `footing` marks an answer `Grounded` when a tool that only reads came back,
 * and `Grounded` promises *there is a source and you can look at it again*.
 * Until the citations, that promise was kept only by retrieval chips — so an
 * answer built entirely out of a web page was marked grounded and pointed at
 * nothing. The state was right and the sentence it produced was not, which is
 * this codebase's recurring failure.
 */
describe('citing a page', () => {
  it('opens in a browser, not in the note editor', () => {
    expect(app).toContain('source.node_type === WEB_SOURCE');
    expect(app).toContain('openBeside(source.id)');
    expect(types).toContain("export const WEB_SOURCE = 'web'");
  });

  /**
   * It used to go to the user's own browser, because checking a citation
   * inside the app would be reading Syn's copy rather than the source.
   *
   * That reasoning was right and its premise has changed. The pane is not
   * Syn's copy: it is a live browser with its own cookie jar, fetching the
   * page as the person and showing them the address it is on. And it is the
   * same door as a link in the answer above the chip, which look identical to
   * whoever clicks them.
   *
   * `openBeside` still reaches their own browser wherever there is no pane to
   * put a page in — a narrow window, and a phone always. That decision is made
   * in Rust, because only Rust can tell "there is no room" from "that address
   * is refused", and those two deserve opposite answers.
   */
  it('says why the pane is not Syn’s copy'.replace('’', "'"), () => {
    expect(app).toContain('It is a live browser with its own session');
    expect(app).toContain('which is what a phone always is');
  });
});
