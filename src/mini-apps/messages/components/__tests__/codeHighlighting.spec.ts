import { describe, it, expect } from 'vitest';
import source from '../MessageBubble.vue?raw';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import css from 'highlight.js/lib/languages/css';
import markdown from 'highlight.js/lib/languages/markdown';
import xml from 'highlight.js/lib/languages/xml';
import json from 'highlight.js/lib/languages/json';

/**
 * A code block whose language nobody named must not cost a language contest.
 *
 * # What is proven, and what is not
 *
 * Proven by intervention, in a browser, on the real conversation: with
 * `highlightAuto` the renderer stops responding at 40 messages; without it the
 * same 40 render in 54MB and stay responsive. Removing the call is what fixed
 * it.
 *
 * Not proven: *why* one call is enough. Timed in isolation it costs 28ms in
 * Node and a fraction of that in jsdom — expensive for what it buys, nowhere
 * near a freeze. So either the browser's regex engine is far slower on these
 * grammars, or the callback runs many more times than once. That question is
 * left open here rather than answered by assertion, because guessing at
 * mechanisms is what cost the four wrong diagnoses below.
 *
 * `highlightAuto` runs every registered grammar over the text to guess which
 * one it is. One assistant answer of 3,797 characters containing a single
 * ```text block was enough to leave the WebView unresponsive with the message
 * half-drawn — sixteen seconds after the Rust side had finished and written
 * the whole reply to disk.
 *
 * It took four wrong diagnoses to find. Markdown parsing was measured and
 * cleared at 2.26ms; the notification count was measured and cleared at 157
 * bubbles; the polling loop was read and cleared; a five-minute HTTP timeout
 * was real but was a *different* freeze. What found it was bisecting a real
 * conversation one message at a time in a browser: 39 messages rendered, 40
 * hung, and the fortieth was the only one in the whole conversation with a
 * code fence.
 *
 * The earlier benchmark missed it for an instructive reason. It reimplemented
 * the highlight callback from memory and wrote the *safe* version — falling
 * back to a single language rather than to auto-detection — so it measured
 * code that was never running. A measurement of something you wrote yourself
 * measures your intentions.
 */
describe('highlighting a code block', () => {
  it('never asks highlight.js to guess the language', () => {
    // The call, not the word: the comment above the fix names it, and an
    // `includes` check failed on its own explanation.
    expect(
      /hljs\s*\.\s*highlightAuto\s*\(/.test(source),
      'highlightAuto runs every registered grammar over the block to guess which ' +
        'one it is. Removing that call is what made a real 40-message conversation ' +
        'render at all.',
    ).toBe(false);
  });

  it('does not offer highlight.js its markdown grammar', () => {
    // The grammar hangs this renderer. A 292-character daily-note template —
    // headings and `- [ ]` items, exactly what the assistant writes when asked
    // for one — froze the page indefinitely. The same grammar on the same input
    // takes 2ms in Node, so it is a regex that backtracks catastrophically in
    // one engine and not the other, and the browser is the one that ships.
    //
    // Established by holding the conversation constant and retagging every
    // fence: `json` rendered in 38MB and stayed responsive, no language at all
    // rendered fine, the block removed rendered fine, `markdown` hung. After
    // the grammar was dropped, retagging everything as `markdown` renders in
    // 24MB with both blocks visible.
    //
    // Neither a size cap nor a try/catch would help: 292 characters is already
    // small, and it does not throw — it does not return.
    for (const forbidden of ["registerLanguage('markdown'", "registerLanguage('md'"]) {
      expect(
        source.includes(forbidden),
        `${forbidden}…) puts the grammar back. A fenced markdown block then hangs ` +
          'the message list, and the conversation that contains it cannot be opened.',
      ).toBe(false);
    }

    // And it must not creep back in through the import either.
    expect(/from\s+'highlight\.js\/lib\/languages\/markdown'/.test(source)).toBe(false);
  });

  it('escapes an unhighlighted block, because the result is inserted as HTML', () => {
    // `markedHighlight` treats the return value as markup. Handing back the
    // source unescaped would render `<b>` inside a code block as bold.
    const escape = source.split('const asPlainCode')[1]?.split('marked.use')[0] ?? '';
    expect(escape, 'MessageBubble should still declare asPlainCode').toBeTruthy();
    for (const entity of ['&amp;', '&lt;', '&gt;', '&quot;']) {
      expect(escape).toContain(entity);
    }
  });

  /**
   * The measurement the fix rests on, kept so the number can be re-read rather
   * than remembered. Not an assertion on timing — a loaded CI box is not this
   * laptop — but a printed figure and a hard ceiling far above any healthy one.
   */
  it('is far cheaper than guessing, on the block that caused it', () => {
    for (const [name, lang] of [
      ['javascript', javascript], ['css', css], ['markdown', markdown],
      ['xml', xml], ['json', json],
    ] as const) {
      hljs.registerLanguage(name, lang);
    }

    // The real block: a directory tree, which is what a model draws when asked
    // to propose a vault layout.
    const block = [
      'Inbox', '├── Ý tưởng mới', '├── Việc phát sinh', '└── Link chưa xử lý', '',
      'Projects', '├── PSS Operations', '├── Monitoring & Alerting', '└── Synabit', '',
      'Knowledge', '├── Network', '├── Database', '└── Security',
    ].join('\n');

    const time = (fn: () => void) => {
      fn();
      const started = performance.now();
      for (let i = 0; i < 5; i++) fn();
      return (performance.now() - started) / 5;
    };

    const guessing = time(() => hljs.highlightAuto(block));
    const plain = time(() => block.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`));

    console.log(`\n── ${block.length} chars: highlightAuto ${guessing.toFixed(1)}ms · plain ${plain.toFixed(3)}ms ──\n`);

    expect(guessing).toBeGreaterThan(plain);
    expect(
      plain,
      'showing a block as itself should be effectively free',
    ).toBeLessThan(2);
  });
});
