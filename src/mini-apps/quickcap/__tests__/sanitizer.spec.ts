import { describe, it, expect } from 'vitest';
import DOMPurify from 'dompurify';

/**
 * The card preview is built as an HTML string and then sanitised, so
 * everything it needs has to survive that pass. Three things do work today
 * and would fail silently if the configuration were tightened: a checklist
 * item carries `data-task`, which is how a click maps back to a line; an
 * audio player carries `controls` and `preload`; and both point at
 * `asset://` URLs the default policy would drop.
 *
 * Silently is the problem. Nothing throws — the attribute simply vanishes and
 * ticking a box stops doing anything.
 */
const CONFIG = {
  ADD_ATTR: ['target', 'controls', 'preload'],
  ADD_URI_SAFE_ATTR: ['preload'],
  ALLOWED_URI_REGEXP: /^(?:(?:https?|asset):)|(?:data:image\/)/i,
};

const clean = (html: string) => DOMPurify.sanitize(html, CONFIG);

describe('the preview sanitiser', () => {
  it('keeps the attribute a checklist click depends on', () => {
    const out = clean('<span data-task="2">mua sữa</span>');
    expect(out).toContain('data-task="2"');
  });

  it('keeps an audio player playable', () => {
    const out = clean('<audio controls preload="none" src="asset://localhost/x.webm"></audio>');
    expect(out).toContain('controls');
    expect(out).toContain('preload');
    expect(out).toContain('asset://');
  });

  it('keeps images served over the asset protocol', () => {
    expect(clean('<img src="asset://localhost/a.png" />')).toContain('asset://');
  });

  /** The reason any of this is sanitised at all. */
  it('still removes what a cap must never be able to run', () => {
    expect(clean('<script>alert(1)</script>')).not.toContain('alert');
    expect(clean('<img src=x onerror="alert(1)" />')).not.toContain('onerror');
    expect(clean('<a href="javascript:alert(1)">x</a>')).not.toContain('javascript:');
  });
});
