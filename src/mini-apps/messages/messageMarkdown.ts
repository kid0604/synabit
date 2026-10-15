/**
 * How an answer's markdown becomes HTML: highlighted code, Mermaid left for
 * the diagram renderer, mathematics left for KaTeX.
 *
 * # Why a module, and its own `Marked`
 *
 * This used to be configured inside `MessageBubble`'s `<script setup>`, on the
 * `marked` every other screen shares. `<script setup>` runs once per bubble,
 * so each message drawn added another highlighter, another renderer and
 * another math extension to the global — and `marked` runs every highlighter
 * it has been given, each over the previous one's HTML. A conversation opened
 * after a few others highlighted its code blocks hundreds of times, every pass
 * escaping the last: `sudo xattr …` came out as `&amp;amp;amp;…`, and on
 * 2026-10-15 a click on a conversation with three `bash` blocks held the
 * WebView at full CPU and 5 GB until it was killed.
 *
 * It is also what the earlier freezes were. `highlightAuto` "hanging at forty
 * messages" and the markdown grammar "hanging on 292 characters" were each one
 * grammar run once per bubble ever drawn, over its own growing output — which
 * is why neither could be reproduced in Node, where one call is one call. The
 * two guards they left are kept: both grammars are still a poor trade.
 *
 * Here the setup runs once, on an instance of its own, so nothing the notes
 * export, the whiteboard or the file viewer parse is highlighted or rendered
 * by the chat's rules either.
 */
import { Marked, Renderer, type Tokens } from 'marked';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import json from 'highlight.js/lib/languages/json';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import sql from 'highlight.js/lib/languages/sql';
import { mathExtension } from './markdownMath';

hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('json', json);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('sql', sql);
// `markdown` and `md` are deliberately NOT registered.
//
// highlight.js's markdown grammar hangs this renderer. A 292-character block —
// a daily-note template of headings and `- [ ]` items, which is exactly what
// the assistant produces when asked for one — froze the page indefinitely.
// The same grammar on the same input takes 2ms in Node, so this is a regex
// that backtracks catastrophically in one engine and not the other; the
// browser is the one that matters here.
//
// Established by intervention, not inference. Holding the conversation
// constant and retagging every fence: `json` renders in 38MB and stays
// responsive, no language at all renders fine, the block removed renders fine,
// and `markdown` hangs. Every arm was run twice.
//
// A size cap would not have helped — 292 characters is already small. Nor
// would a `try`/`catch`: it does not throw, it does not return. Not offering
// the grammar is the only guard that works, and the cost is that a fenced
// markdown block is shown unhighlighted, which for markdown inside markdown is
// close to no cost at all.

// Custom renderer to intercept mermaid code blocks
const renderer = new Renderer();
const originalCodeRenderer = renderer.code;
let mermaidIdCounter = 0;

renderer.code = function (token: Tokens.Code) {
  if (token.lang === 'mermaid') {
    const id = `mermaid-${Date.now()}-${mermaidIdCounter++}`;
    return `<div class="mermaid-container"><pre class="mermaid" id="${id}">${token.text}</pre></div>`;
  }
  // Fall back to original renderer for non-mermaid code
  return originalCodeRenderer.call(this, token);
};

/**
 * A code block as literal text, with the four characters that would otherwise
 * be markup taken out of play.
 *
 * `markedHighlight` treats whatever `highlight` returns as HTML, so the
 * unhighlighted path cannot hand back the source unescaped: a block containing
 * `<b>` would render as bold rather than as itself. DOMPurify would still keep
 * it safe; it would just be wrong.
 */
const asPlainCode = (code: string): string =>
  code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

const highlight = markedHighlight({
  langPrefix: 'hljs language-',
  highlight(code, lang) {
    if (lang === 'mermaid') return code; // Don't highlight mermaid — it's handled by the renderer
    if (lang && hljs.getLanguage(lang)) {
      // `ignoreIllegals`, because a block half-written by a stream is not yet
      // valid in its own language and throwing over that is not useful.
      return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
    }

    // Deliberately no `highlightAuto`.
    //
    // It ran every registered grammar over the block to guess which language
    // it was, and that guess is both expensive and, for anything that
    // resembles no language, wrong — on a directory tree drawn with `├──` it
    // answered "css". One 3,797-character answer containing a single
    // ```text block was enough to leave the WebView unresponsive with the
    // message half-drawn, while the Rust side had been finished for sixteen
    // seconds.
    //
    // Found by bisecting a real conversation one message at a time in a
    // browser: 39 messages rendered fine, 40 hung, and the fortieth was the
    // only one in the whole conversation with a code fence.
    //
    // A block whose language nobody named is shown as what it is.
    return asPlainCode(code);
  },
});

// Mathematics. Registered after the renderer so its own `renderer` entries are
// the ones that answer for these tokens. See `markdownMath` for why all four
// delimiters are inline-level and why the TeX waits in an attribute.
export const messageMarked = new Marked(
  highlight,
  { renderer },
  mathExtension,
  { breaks: true, gfm: true },
);
