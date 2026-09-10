import { describe, it } from 'vitest';
import { marked, Marked } from 'marked';
import { mathExtension, MATH_ATTRS, renderMathIn } from '../../markdownMath';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import DOMPurify from 'dompurify';

// The real answer that left the WebView at 62% CPU, kept beside this spec so
// the measurement is of something that actually happened.
import answer from './slow-answer.txt?raw';

/**
 * What it costs to draw a streaming answer, measured rather than guessed.
 *
 * A real answer of 3,797 characters left the WebView process at 62% CPU with
 * the message half-drawn and the stop button still showing, while the Rust
 * side had finished sixteen seconds earlier and written the whole reply to
 * disk. Two obvious culprits were checked by reading the code and both were
 * already handled: the markdown re-render is debounced to 100ms and the
 * auto-scroll is throttled to the same. So the cost is somewhere else, and
 * guessing where is how the last three fixes in this session went wrong.
 *
 * This measures the half that can be measured without a browser: turning
 * markdown into sanitised HTML, at the sizes and the cadence a stream produces.
 * Layout, reflow and the smooth-scroll animation cannot be measured in jsdom
 * and are deliberately not claimed about here.
 *
 * ```bash
 * npx vitest run streamRenderCost --reporter=verbose
 * ```
 */
describe('drawing a streamed answer', () => {
  hljs.registerLanguage('javascript', javascript);
  hljs.registerLanguage('bash', bash);
  hljs.registerLanguage('json', json);

  // This callback must match `MessageBubble`'s. The first version of this
  // benchmark did not: it reimplemented the fallback from memory and wrote the
  // *safe* one — a single language rather than auto-detection — so it measured
  // code that was never running and reported 2.26ms for a render that in fact
  // hung the browser. A benchmark of something you wrote yourself measures
  // your intentions.
  marked.use(
    markedHighlight({
      langPrefix: 'hljs language-',
      highlight(code, lang) {
        if (lang === 'mermaid') return code;
        if (lang && hljs.getLanguage(lang)) {
          return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
        }
        return code.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
      },
    }),
  );
  marked.setOptions({ breaks: true, gfm: true });

  /** One pass of what `renderedContent` does. */
  const render = (text: string) => {
    const html = marked.parse(text) as string;
    return DOMPurify.sanitize(html, {
      ADD_TAGS: ['pre', 'code', 'svg', 'g', 'path', 'rect', 'circle', 'line', 'text'],
      ADD_ATTR: ['class', 'id', 'viewBox', 'd', 'fill', 'stroke', 'style'],
    });
  };

  const timed = (times: number, fn: () => void) => {
    fn(); // warm
    const started = performance.now();
    for (let i = 0; i < times; i++) fn();
    return (performance.now() - started) / times;
  };

  it('costs this much per pass, at these lengths', () => {
    console.log('\n── one render pass ─────────────────────────────');
    console.log('chars      parse+sanitize   parse only   sanitize only');

    for (const fraction of [0.1, 0.25, 0.5, 1]) {
      const slice = answer.slice(0, Math.floor(answer.length * fraction));
      const both = timed(20, () => render(slice));
      const parseOnly = timed(20, () => marked.parse(slice));
      const html = marked.parse(slice) as string;
      const sanitizeOnly = timed(20, () => DOMPurify.sanitize(html));
      console.log(
        `${String(slice.length).padEnd(10)} ${both.toFixed(2).padStart(11)}ms ${parseOnly
          .toFixed(2)
          .padStart(11)}ms ${sanitizeOnly.toFixed(2).padStart(12)}ms`,
      );
    }
  });

  /**
   * What accepting mathematics added to that.
   *
   * Measured because it is the one thing in this pipeline that runs on every
   * pass of a stream and does real work — KaTeX turns each formula into a small
   * forest of spans, and unlike Mermaid it cannot wait for the answer to finish
   * (a half-written formula is still text; a half-written diagram is an error).
   *
   * The answer here is deliberately almost nothing but formulas, so this is a
   * ceiling and not a typical message. An answer with no mathematics in it pays
   * one `querySelectorAll` that finds nothing.
   */
  it('costs this much more when the answer is mathematics', () => {
    const withMath = new Marked();
    withMath.use(mathExtension);

    const maths = `Đạo hàm hàm hợp là $f'(g(x))g'(x)$, và tích phân từng phần cho

$$\\int u\\,dv = uv - \\int v\\,du$$

Chuỗi Fourier: $c_n = \\frac{1}{T}\\int_0^T f(t)e^{-i\\omega_n t}dt$ với
$\\omega_n = 2\\pi n / T$, và tổng riêng phần

$$S_N(t) = \\sum_{n=-N}^{N} c_n e^{i\\omega_n t}$$

hội tụ theo $O(1/N)$ với hàm khả vi từng khúc.

`.repeat(3);

    console.log('\n── mathematics, per pass ───────────────────────');
    console.log('chars      markdown      katex   formulas');

    for (const fraction of [0.25, 0.5, 1]) {
      const slice = maths.slice(0, Math.floor(maths.length * fraction));
      const markdown = timed(20, () => withMath.parse(slice));

      const html = DOMPurify.sanitize(withMath.parse(slice) as string, {
        ADD_ATTR: [...MATH_ATTRS],
      });
      const host = document.createElement('div');
      let formulas = 0;
      const katexMs = timed(20, () => {
        host.innerHTML = html;
        formulas = renderMathIn(host);
      });

      console.log(
        `${String(slice.length).padEnd(10)} ${markdown.toFixed(2).padStart(7)}ms ` +
          `${katexMs.toFixed(2).padStart(9)}ms ${String(formulas).padStart(9)}`,
      );
    }
  });

  /**
   * The whole stream, as the 100ms debounce actually drives it: one render for
   * each window in which at least one token arrived, over a growing prefix.
   */
  it('costs this much for a whole answer, at the debounce it has', () => {
    for (const debounceMs of [100, 250, 0]) {
      // A 16-second stream, which is what the observed answer took.
      const streamMs = 16_000;
      const passes = debounceMs === 0 ? 400 : Math.floor(streamMs / debounceMs);

      let total = 0;
      for (let i = 1; i <= passes; i++) {
        const slice = answer.slice(0, Math.ceil((answer.length * i) / passes));
        const started = performance.now();
        render(slice);
        total += performance.now() - started;
      }
      console.log(
        `debounce ${String(debounceMs).padStart(4)}ms → ${passes} passes, ` +
          `${total.toFixed(0)}ms of rendering across a ${streamMs / 1000}s stream ` +
          `(${((total / streamMs) * 100).toFixed(1)}% of one core)`,
      );
    }
    console.log('');
  });
});
