import { describe, it, expect } from 'vitest';
import { titleFor, bodyFor, KEPT_IN } from '../keepAsNote';
import bubble from '../components/MessageBubble.vue?raw';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

const FALLBACK = 'Diagram from Syn';

/**
 * A note nobody can find by name is a note that was not really kept.
 *
 * So the name comes from the best thing somebody actually wrote, and never from
 * a counter: "Diagram 3" is a filename, not a title.
 */
describe('naming what gets kept', () => {
  it('takes the diagram’s own title when it has one', () => {
    expect(titleFor('---\ntitle: Kiến trúc Splunk\n---\nflowchart LR\n A-->B', '', FALLBACK))
      .toBe('Kiến trúc Splunk');
  });

  /** Mermaid takes a title two ways, and `pie` uses the bare one. */
  it('takes the bare title a pie or an xychart carries', () => {
    expect(titleFor('pie title Monthly Spending\n "Food" : 45', '', FALLBACK))
      .toBe('Monthly Spending');
    expect(titleFor('xychart-beta\ntitle "Income vs Expense"\nbar [1,2]', '', FALLBACK))
      .toBe('Income vs Expense');
  });

  /** An answer that draws something usually says what it is drawing first. */
  it('falls back to the heading above it in the answer', () => {
    const around = 'Đây là sơ đồ:\n\n## Kiến trúc hệ thống\n\nnhìn vào đây';
    expect(titleFor('flowchart LR\n A-->B', around, FALLBACK)).toBe('Kiến trúc hệ thống');
  });

  it('falls back to what was asked, and only then', () => {
    expect(titleFor('flowchart LR\n A-->B', 'no headings here', FALLBACK)).toBe(FALLBACK);
  });

  /**
   * Quotes and emphasis off; punctuation left alone.
   *
   * `create_node_file` names the file after a UUID and the title lives in the
   * frontmatter, so there is nothing to protect a filename from — and a rule
   * that stripped `:` would take the colon out of *Kiến trúc: Splunk* for
   * nothing.
   */
  it('takes off the quoting, and leaves the punctuation', () => {
    expect(titleFor('xychart-beta\ntitle "Income vs Expense"', '', FALLBACK))
      .toBe('Income vs Expense');
    expect(titleFor('---\ntitle: "**Bold** thing"\n---', '', FALLBACK)).toBe('Bold thing');
    expect(titleFor('pie title Kiến trúc: Splunk', '', FALLBACK)).toBe('Kiến trúc: Splunk');
  });

  it('does not hand back a title longer than a sidebar row can show', () => {
    const long = 'x'.repeat(200);
    expect(titleFor(`pie title ${long}`, '', FALLBACK).length).toBe(80);
  });
});

/**
 * The person asked for the diagram, not for a receipt. A note that opens with
 * provenance instead of content is a note that has to be scrolled past before
 * it can be read.
 */
describe('what gets written', () => {
  it('is the diagram, in a fence, and nothing else', () => {
    expect(bodyFor('  flowchart LR\n  A-->B  ')).toBe('```mermaid\nflowchart LR\n  A-->B\n```\n');
  });

  it('goes where notes go', () => {
    expect(KEPT_IN).toBe('Notes');
  });
});

/**
 * The first door out of a conversation.
 *
 * The panel sits inside a vault app full of surfaces and could reach none of
 * them: Syn writes to the vault through its tools, but the answer it hands back
 * was a dead end. This is the shape a table, an image or a sketch will reuse.
 */
describe('the button on the block', () => {
  it('writes a real note rather than pretending to', () => {
    expect(bubble).toContain('nodes.createNode(');
    expect(bubble).toContain('nodes.writeNode(');
    expect(bubble).toContain('directory: KEPT_IN');
  });

  /**
   * A button inside a button is a click nobody can predict and a thing no
   * screen reader can describe. The diagram opens the viewer; the actions sit
   * outside it.
   */
  it('keeps its buttons out of the control that opens the viewer', () => {
    const rendered = bubble.slice(bubble.indexOf('const opened ='), bubble.indexOf('const container'));
    expect(rendered).toContain('mermaid-rendered');
    expect(rendered).toContain('mermaid-actions');

    const openArea = rendered.slice(0, rendered.indexOf('mermaid-actions'));
    expect(openArea, 'the action bar is a sibling, not a child').toContain('</div>');
  });

  /**
   * Keeping something and going to look at it are two decisions, and only the
   * first one was made. So the button changes rather than the screen.
   */
  it('offers to open what it kept, instead of jumping there', () => {
    expect(bubble).toContain("button.dataset.act = 'open'");
    expect(bubble).toContain("emit('open-source', kept)");
    expect(bubble, 'no navigation behind the reader’s back').not.toContain("router.push");
  });

  /**
   * A note kept from here can be deleted anywhere — the vault is one thing and
   * this panel is a view of it.
   *
   * Left alone, the button went on claiming a note that was gone, and pressing
   * it handed the reader to an editor opening a file that is not there. That
   * does not fail; it waits. The reader gets a spinner that never stops.
   */
  it('forgets a note that has been deleted, and offers to keep it again', () => {
    expect(bubble).toContain("bus.on('node:deleted'");
    expect(bubble).toContain('forgetKept');
    expect(bubble, 'and back to what it can still do').toContain("t('syn.keep_as_note')");
  });

  /**
   * The bus only knows about deletions this window saw. A note trashed on
   * another device and synced in, or in a session before this one, is gone
   * without any event — so the note is asked for before the reader is sent to
   * it.
   */
  it('checks the note is still there before going to it', () => {
    const open = bubble.slice(bubble.indexOf("if (button.dataset.act === 'open')"));
    const body = open.slice(0, open.indexOf('\n  }'));
    expect(body).toContain('nodes.getNode(kept.id)');
    expect(body).toContain('forgetKept(id)');
  });

  /** There is no toast system here, and a control that silently does nothing
   *  is worse than one that admits it. */
  it('says on itself when it could not', () => {
    expect(bubble).toContain("t('syn.keep_failed')");
  });

  it('has words in both languages', () => {
    for (const locale of [en, vi]) {
      for (const key of ['keep_as_note', 'keep_open_it', 'keep_failed', 'keep_untitled']) {
        expect(locale.syn, key).toHaveProperty(key);
      }
    }
    expect(en.syn.keep_open_it).toContain('{title}');
    expect(vi.syn.keep_open_it).toContain('{title}');
  });

  /**
   * The line from `docs/syn-the-conversation-2026-09-10.md` §2: the model
   * describes content, never behaviour. This button is the app's, decided by
   * what the block is — nothing in an answer can ask for one.
   */
  it('is the app’s button and not one the answer asked for', () => {
    const source = bubble.slice(0, bubble.indexOf('<template>'));
    expect(source).toContain("data-act=");
    expect(
      source,
      'an action read out of the message would be a web page acting through the reader',
    ).not.toMatch(/data-act="\$\{[^}]*token/);
  });
});

/**
 * How wide an answer is allowed to be.
 *
 * A four-column table or a Mermaid flowchart squeezed into 48rem is not narrow,
 * it is unreadable — and several hundred pixels sat empty beside it. So the
 * column widens and the *prose* keeps the measure it already had: the width
 * goes to the things that need it, not to the sentences.
 */
describe('the room an answer gets', () => {
  it('gives the column more than the measure of a sentence', async () => {
    const panel = (await import('../components/ChatPanel.vue?raw')).default;
    expect(panel, 'the messages').toContain('max-w-5xl mx-auto flex flex-col gap-5');
    expect(panel, 'and the composer under them, or the two sit off-centre')
      .toContain('max-w-5xl mx-auto');
    expect(panel, 'nothing is still pinned to the old column').not.toContain('max-w-3xl');
  });

  /**
   * An answer takes the column; a question keeps to its own side. Capping the
   * answer at 80% of a wider column would put the width back where it was and
   * leave the gap where it was too.
   */
  it('lets an answer use it, and leaves a question where it was', () => {
    const content = bubble.slice(bubble.indexOf('<!--\n      Content.'), bubble.indexOf('<!-- Bubble -->'));
    expect(content).toContain("'flex flex-col items-end max-w-[80%]' : 'flex-1'");
  });

  /**
   * 76ch is about 590 pixels at this font size, and a paragraph here was 582 —
   * 80% of a 48rem column less the card's padding. Prose reads as it did; only
   * the room around it changed.
   */
  it('keeps prose to a measure, and only prose', () => {
    expect(bubble).toContain('max-width: 76ch');

    const measure = bubble.slice(bubble.indexOf(':deep(.prose > p)'), bubble.indexOf('max-width: 76ch'));
    for (const tag of ['p', 'ul', 'ol', 'blockquote', 'h2']) {
      expect(measure, `${tag} is text and keeps the measure`).toContain(`:deep(.prose > ${tag})`);
    }
    for (const wide of ['table', 'pre', 'img', '.mermaid-container']) {
      expect(measure, `${wide} is why the column is wide`).not.toContain(`> ${wide})`);
    }
  });

  /** The column is wide now; this is the backstop for the tables that are
   *  wider still. */
  it('scrolls a table rather than crushing its columns', () => {
    const rule = bubble.slice(bubble.indexOf(':deep(.prose table)'));
    expect(rule.slice(0, 160)).toContain('overflow-x: auto');
  });
});
