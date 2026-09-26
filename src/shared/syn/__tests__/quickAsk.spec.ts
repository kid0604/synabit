import { describe, it, expect } from 'vitest';

import { nextMode, quickEntryAction, routeQuickQuestion } from '../quickAsk';

/**
 * The quick-entry box, asked two different things by one key.
 *
 * The rule these protect is that a thought typed into the box is never lost
 * to Syn: capture is the default, asking is chosen, and anything that makes a
 * question impossible turns it back into a capture rather than into nothing.
 */
describe('what Enter does in the quick-entry box', () => {
  it('captures by default', () => {
    expect(quickEntryAction('capture', 'mua sữa', true)).toEqual({ kind: 'capture', text: 'mua sữa' });
  });

  /**
   * A question is a perfectly good capture. The box does not guess from the
   * text — only the mode decides.
   */
  it('captures a question when capture is chosen', () => {
    expect(quickEntryAction('capture', 'cuốn sách Lan nhắc tên gì?', true)?.kind).toBe('capture');
    expect(quickEntryAction('capture', '? giá per-seat', true)?.kind).toBe('capture');
  });

  it('asks when ask is chosen', () => {
    expect(quickEntryAction('ask', '  tuần này tôi hứa gì với Lan? ', true)).toEqual({
      kind: 'ask',
      text: 'tuần này tôi hứa gì với Lan?',
    });
  });

  /** Syn switched off after the mode was picked: the words are still kept. */
  it('captures instead when asking is no longer possible', () => {
    expect(quickEntryAction('ask', 'hỏi gì đó', false)).toEqual({ kind: 'capture', text: 'hỏi gì đó' });
  });

  it('does nothing with an empty box', () => {
    expect(quickEntryAction('capture', '   \n', true)).toBeNull();
    expect(quickEntryAction('ask', '', true)).toBeNull();
  });
});

describe('switching between capture and ask', () => {
  it('flips each time', () => {
    expect(nextMode('capture', true)).toBe('ask');
    expect(nextMode('ask', true)).toBe('capture');
  });

  /** Without Syn there is only one thing the box can be. */
  it('stays on capture when Syn is not available', () => {
    expect(nextMode('capture', false)).toBe('capture');
    expect(nextMode('ask', false)).toBe('capture');
  });
});

describe('a question arriving at the main window', () => {
  it('opens the bar when the bar is allowed', () => {
    expect(routeQuickQuestion({ hasVault: true, synEnabled: true, allowed: true })).toBe('open');
  });

  /**
   * Locked: the words stay in Rust until the PIN, rather than being pulled into
   * a window that is promising to show nothing — and rather than being lost.
   */
  it('waits behind a lock', () => {
    expect(routeQuickQuestion({ hasVault: true, synEnabled: true, allowed: false })).toBe('wait');
  });

  it('becomes a capture when there is no Syn to ask', () => {
    expect(routeQuickQuestion({ hasVault: true, synEnabled: false, allowed: false })).toBe('capture');
    expect(routeQuickQuestion({ hasVault: false, synEnabled: true, allowed: false })).toBe('capture');
  });
});
