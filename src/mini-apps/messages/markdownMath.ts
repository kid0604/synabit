/**
 * Mathematics, in a conversation.
 *
 * # Why this was the first gap worth closing
 *
 * Because it is the only rich format that costs **nothing in the prompt**. Syn
 * is already told, in four hundred and thirty characters sent on every single
 * turn, how to draw a chart with Mermaid. Nobody has to be told to write
 * `$$\sum_{i=1}^{n}$$` — every model of this generation writes TeX for
 * mathematics by reflex, unprompted, and always has.
 *
 * So the chat has been receiving mathematics all along and printing it as
 * `$$\sum_{i=1}^{n}$$`. Not degraded: wrong, in the most visible way there is.
 *
 * # Why a module and not a few lines in the bubble
 *
 * Because the interesting part is a handful of regular expressions deciding
 * what is and is not mathematics, and that decision is worth testing on its
 * own. `$5 and $10` is not a formula, and a renderer that thinks it is has
 * made every price in every answer unreadable to fix something nobody asked
 * for.
 */
import katex from 'katex';
import type { MarkedExtension, Tokens } from 'marked';

/** The attributes the placeholder carries, which `DOMPurify` has to keep. */
export const MATH_ATTRS = ['data-tex', 'data-display'];

/** What a placeholder looks like before KaTeX has been near it. */
export const MATH_MARK = 'data-tex';

const escapeAttr = (raw: string): string =>
  raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

/**
 * The formula, parked rather than rendered.
 *
 * # Why the TeX waits in an attribute instead of becoming KaTeX here
 *
 * Because everything this renderer produces goes through `DOMPurify` with an
 * explicit allowlist, and KaTeX's output is a forest of spans and MathML that
 * would mean widening that list a long way — for markup generated from text a
 * model wrote after reading somebody's web page.
 *
 * An attribute is inert. It survives sanitising as *data*, and KaTeX runs
 * afterwards on the result, writing into an element the sanitiser has already
 * approved. Same shape as the Mermaid path directly above it in the bubble,
 * and for the same reason.
 */
const parked = (tex: string, display: boolean): string =>
  `<span class="syn-math${display ? ' syn-math-display' : ''}"` +
  ` data-display="${display}" data-tex="${escapeAttr(tex)}"></span>`;

/**
 * Whether what sits between two single dollars is mathematics or money.
 *
 * Three rules, and each one is a real sentence somebody has written:
 *
 * * **No space against either dollar.** `$ x $` is not a formula; a lone `$`
 *   in prose almost never has its partner tight against a word.
 * * **No newline.** Inline mathematics does not wrap; a `$` at the end of one
 *   line and another two paragraphs later is two prices.
 * * **No digit straight after the closing dollar.** This is the one that saves
 *   `$5 and $10`: without it, `5 and ` is a formula and both prices vanish.
 */
const looksLikeMath = (body: string): boolean =>
  body.length > 0 && !/^\s/.test(body) && !/\s$/.test(body) && !body.includes('\n');

/**
 * The four ways a model writes mathematics, and no fifth.
 *
 * `$$…$$` and `$…$` are what the OpenAI-family models emit by default;
 * `\[…\]` and `\(…\)` are what they emit when asked for LaTeX, and what
 * Anthropic's models lean towards. All four are here because which one arrives
 * is a property of a provider that can change under this app without notice.
 *
 * Inline level, all of them, including the display pair. A block-level
 * tokenizer runs **before** marked's own, which would let a `$$` inside a
 * fenced code block win against the fence — and a fenced block is exactly
 * where somebody demonstrates the syntax. Inline tokenizers never see the
 * inside of a fence at all.
 */
export const mathExtension: MarkedExtension = {
  extensions: [
    {
      name: 'mathDisplay',
      level: 'inline',
      start: (src: string) => {
        const dollars = src.indexOf('$$');
        const bracket = src.indexOf('\\[');
        if (dollars < 0) return bracket < 0 ? undefined : bracket;
        return bracket < 0 ? dollars : Math.min(dollars, bracket);
      },
      tokenizer(src: string) {
        const m = /^\$\$([\s\S]+?)\$\$/.exec(src) ?? /^\\\[([\s\S]+?)\\\]/.exec(src);
        if (!m) return undefined;
        return { type: 'mathDisplay', raw: m[0], text: m[1].trim() };
      },
      renderer: (token: Tokens.Generic) => parked(String(token.text), true),
    },
    {
      name: 'mathInline',
      level: 'inline',
      start: (src: string) => {
        const dollar = src.indexOf('$');
        const paren = src.indexOf('\\(');
        if (dollar < 0) return paren < 0 ? undefined : paren;
        return paren < 0 ? dollar : Math.min(dollar, paren);
      },
      tokenizer(src: string) {
        const paren = /^\\\(([\s\S]+?)\\\)/.exec(src);
        if (paren) return { type: 'mathInline', raw: paren[0], text: paren[1].trim() };

        const m = /^\$(?!\$)((?:[^$\\]|\\.)+?)\$(?!\d)/.exec(src);
        if (!m || !looksLikeMath(m[1])) return undefined;
        return { type: 'mathInline', raw: m[0], text: m[1] };
      },
      renderer: (token: Tokens.Generic) => parked(String(token.text), false),
    },
  ],
};

/**
 * Turn every parked formula in this element into mathematics.
 *
 * `throwOnError: false` on purpose: KaTeX then draws what it could not parse in
 * red, in place, which tells the reader *and* the person who wrote the prompt
 * something. Throwing would take the rest of the message down with it.
 *
 * `trust: false` is KaTeX's default and is restated because it is the part that
 * matters here — it refuses `\href` and `\includegraphics`, and this TeX was
 * written by a model that has been reading the open web.
 */
export const renderMathIn = (root: ParentNode): number => {
  const parked_ = root.querySelectorAll(`[${MATH_MARK}]:not([data-rendered])`);

  for (const el of parked_) {
    const tex = el.getAttribute(MATH_MARK) ?? '';
    el.setAttribute('data-rendered', 'true');
    if (!tex) continue;

    try {
      katex.render(tex, el as HTMLElement, {
        displayMode: el.getAttribute('data-display') === 'true',
        throwOnError: false,
        trust: false,
        strict: false,
      });
    } catch {
      // KaTeX still throws for a few things `throwOnError` does not cover.
      // The formula as written is a better answer than an empty gap.
      el.textContent = tex;
    }
  }

  return parked_.length;
};
