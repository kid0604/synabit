import { describe, it, expect, beforeAll } from 'vitest';
import { Marked } from 'marked';
import DOMPurify from 'dompurify';
import { mathExtension, MATH_ATTRS, renderMathIn } from '../markdownMath';
import bubble from '../components/MessageBubble.vue?raw';

/** Its own instance: `marked` is a singleton and the bubble configures the
 *  global one. A test that changed it would change the app's renderer. */
const md = new Marked();
beforeAll(() => { md.use(mathExtension); });

const html = (src: string) => md.parse(src) as string;
const texIn = (src: string) =>
  [...html(src).matchAll(/data-tex="([^"]*)"/g)].map(m => m[1]);

/**
 * Mathematics was the one rich format that cost nothing to accept.
 *
 * Syn is told how to draw a Mermaid chart in four hundred and thirty
 * characters, sent on every turn. Nobody has to be told to write TeX — every
 * model of this generation does it by reflex. So the chat had been receiving
 * mathematics all along and printing `$$\sum_{i=1}^{n}$$` at the reader.
 */
describe('finding mathematics in an answer', () => {
  it('takes all four delimiters, because which one arrives is the provider’s choice', () => {
    expect(texIn('inline $x^2$ here')).toEqual(['x^2']);
    expect(texIn('inline \\(x^2\\) here')).toEqual(['x^2']);
    expect(texIn('$$x^2$$')).toEqual(['x^2']);
    expect(texIn('\\[x^2\\]')).toEqual(['x^2']);
  });

  it('knows which of them is a display formula', () => {
    expect(html('$$x$$')).toContain('data-display="true"');
    expect(html('$x$')).toContain('data-display="false"');
  });

  /**
   * The rule that earns this file its existence.
   *
   * Without the digit guard, `$5 and $10` is a formula reading `5 and ` and
   * both prices disappear from the answer — a renderer breaking something
   * nobody asked it to touch, to fix something else.
   */
  it('leaves money alone', () => {
    expect(texIn('it costs $5 and $10 today')).toEqual([]);
    expect(texIn('between $1,299 and $1,499')).toEqual([]);
    expect(texIn('just $20')).toEqual([]);
  });

  /** A lone dollar in prose almost never has its partner tight against a word. */
  it('wants both delimiters tight against the formula', () => {
    expect(texIn('$ x + y $')).toEqual([]);
    expect(texIn('$x + y$')).toEqual(['x + y']);
  });

  /** Inline mathematics does not wrap. A `$` at the end of one line and
   *  another two paragraphs down is two dollars, not one formula. */
  it('does not let an inline formula span lines', () => {
    expect(texIn('costs $5\n\nand later $6 more')).toEqual([]);
  });

  /**
   * A fenced block is exactly where somebody demonstrates the syntax, and it
   * has to survive being demonstrated. This is why every delimiter is
   * inline-level: a block-level tokenizer runs before marked's own and would
   * win against the fence.
   */
  it('does not touch a formula inside code', () => {
    expect(texIn('```\n$$x^2$$\n```')).toEqual([]);
    expect(texIn('`$x^2$`')).toEqual([]);
  });

  /** Half of a formula is not a formula. This is what makes it safe to run on
   *  every pass of a stream: an unfinished one is still the text it was. */
  it('waits for the closing delimiter', () => {
    expect(texIn('the sum $$\\sum_{i=1}')).toEqual([]);
  });

  /** The TeX travels as data through the sanitiser, so it must arrive as data. */
  it('escapes what it parks in the attribute', () => {
    const out = html('$a < b$');
    expect(out).toContain('data-tex="a &lt; b"');
    expect(out).not.toContain('data-tex="a < b"');
  });
});

describe('drawing it', () => {
  it('renders into the element the sanitiser already approved', () => {
    const host = document.createElement('div');
    host.innerHTML = html('$$E = mc^2$$');
    expect(renderMathIn(host)).toBe(1);
    expect(host.querySelector('.katex')).toBeTruthy();
    expect(host.textContent).not.toContain('data-tex');
  });

  it('does the same formula only once', () => {
    const host = document.createElement('div');
    host.innerHTML = html('$x$ and $y$');
    expect(renderMathIn(host)).toBe(2);
    expect(renderMathIn(host)).toBe(0);
  });

  /**
   * A model writing TeX gets it wrong sometimes, and taking the rest of the
   * answer down over one bad macro would be the worse failure. KaTeX draws
   * what it could not parse, in red, in place.
   */
  it('shows a formula it cannot parse rather than losing the message', () => {
    const host = document.createElement('div');
    host.innerHTML = html('$$\\nonsensemacro{x}$$');
    expect(() => renderMathIn(host)).not.toThrow();
    expect(host.textContent).toBeTruthy();
  });

  /**
   * The formula travels through the sanitiser as data, and the sanitiser has an
   * allowlist. Left off it, `data-tex` is stripped, every formula arrives as an
   * empty span, and the reader is told nothing at all — a whole message quietly
   * missing its mathematics.
   *
   * Asserted against `DOMPurify` itself rather than against the constant,
   * because the constant being right is not the claim. The claim is that a
   * formula survives the trip.
   */
  it('survives the sanitiser it has to pass through', () => {
    const dirty = html('$$E = mc^2$$ and $a < b$');
    const clean = DOMPurify.sanitize(dirty, { ADD_ATTR: [...MATH_ATTRS] });

    const host = document.createElement('div');
    host.innerHTML = clean;

    // Read back off the element rather than out of the string: how an
    // attribute is *spelled* after sanitising is the serialiser's business,
    // and `&lt;` there is the same value as `<`. What matters is what
    // `getAttribute` hands KaTeX.
    const parked = [...host.querySelectorAll('[data-tex]')].map(el => el.getAttribute('data-tex'));
    expect(parked).toEqual(['E = mc^2', 'a < b']);

    expect(renderMathIn(host)).toBe(2);
    expect(host.querySelectorAll('.katex').length).toBe(2);
  });

  /** And the bubble has to actually pass them. A list nobody uses is a list
   *  that is right and does nothing. */
  it('is on the list the bubble hands the sanitiser', () => {
    expect(bubble).toContain('...MATH_ATTRS');
    expect(bubble).toContain("from '../markdownMath'");
    expect(bubble, 'and the stylesheet, or the formula renders unstyled')
      .toContain("import 'katex/dist/katex.min.css'");
  });
});
