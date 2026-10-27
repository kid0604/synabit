import { describe, it, expect, vi } from 'vitest';
import { answer, contextChanged, frameHeight, visualLocale, visualSrc, visualsIn, HEIGHT_CAP, EXPANDED_CAP, MIN_HEIGHT } from '../visualHost';

const context = { theme: 'dark' as const, locale: 'vi-VN' };
const events = () => ({ height: vi.fn(), ask: vi.fn(), error: vi.fn(), initialized: vi.fn() });

describe("the app's answers to a visual's frame", () => {
  it('tells it the theme and the locale when it starts', () => {
    const reply = answer({ jsonrpc: '2.0', id: 1, method: 'ui/initialize', params: {} }, context, events());
    expect(reply).toMatchObject({ jsonrpc: '2.0', id: 1, result: { hostContext: { theme: 'dark', locale: 'vi-VN' } } });
  });

  it('takes a height, and says nothing back', () => {
    const e = events();
    expect(answer({ jsonrpc: '2.0', method: 'ui/notifications/size-changed', params: { height: 322 } }, context, e)).toBeNull();
    expect(e.height).toHaveBeenCalledWith(322);
  });

  /** Decided 2026-10-26 (plan §7): a question a visual asks goes into the box, and nothing here sends it. */
  it('hands a question on for the chat box, trimmed, and answers that it was taken', () => {
    const e = events();
    const reply = answer({ jsonrpc: '2.0', id: 7, method: 'ui/message', params: { role: 'user', content: { type: 'text', text: '  Gấp đôi thì sao?  ' } } }, context, e);
    expect(e.ask).toHaveBeenCalledWith('Gấp đôi thì sao?');
    expect(reply).toEqual({ jsonrpc: '2.0', id: 7, result: {} });
  });

  it('does not hand on an empty question or something that is not text', () => {
    const e = events();
    answer({ jsonrpc: '2.0', id: 8, method: 'ui/message', params: { content: { type: 'text', text: '   ' } } }, context, e);
    answer({ jsonrpc: '2.0', id: 9, method: 'ui/message', params: { content: { type: 'image', data: 'x' } } }, context, e);
    expect(e.ask).not.toHaveBeenCalled();
  });

  it('keeps what broke, bounded', () => {
    const e = events();
    answer({ jsonrpc: '2.0', method: 'synabit/error', params: { kind: 'error', message: 'x'.repeat(2000) } }, context, e);
    expect(e.error).toHaveBeenCalledWith({ kind: 'error', message: 'x'.repeat(500) });
  });

  it('ignores anything that is not JSON-RPC, or a method it does not know', () => {
    const e = events();
    for (const data of [null, 'hi', { method: 'ui/initialize', id: 1 }, { jsonrpc: '2.0', id: 1, method: 'tools/call', params: {} }, { jsonrpc: '1.0', method: 'ui/message' }]) {
      expect(answer(data, context, e)).toBeNull();
    }
    expect(e.ask).not.toHaveBeenCalled();
  });

  it('sends nothing into the frame but the theme and the locale', () => {
    expect(contextChanged(context)).toEqual({
      jsonrpc: '2.0',
      method: 'ui/notifications/host-context-changed',
      params: { theme: 'dark', locale: 'vi-VN' },
    });
  });
});

describe('the frame, sized and addressed', () => {
  it('grows to what it reports, within a cap', () => {
    expect(frameHeight(322, false)).toBe(322);
    expect(frameHeight(5000, false)).toBe(HEIGHT_CAP);
    expect(frameHeight(5000, true)).toBe(EXPANDED_CAP);
    expect(frameHeight(0, false)).toBe(MIN_HEIGHT);
    expect(frameHeight(Number.NaN, false)).toBe(MIN_HEIGHT);
  });

  it('is addressed by id and theme only', () => {
    expect(visualSrc('synvisual://localhost', '0123456789abcdef0123456789abcdef', 'dark')).toBe(
      'synvisual://localhost/v/0123456789abcdef0123456789abcdef?theme=dark',
    );
    expect(visualLocale('en')).toBe('en-US');
    expect(visualLocale('vi')).toBe('vi-VN');
  });

  it("finds an answer's visuals in its tool log, and only real ids", () => {
    const id = '0123456789abcdef0123456789abcdef';
    expect(
      visualsIn([
        { tool_name: 'visual_guide', tool_args: { kind: 'chart' } },
        { tool_name: 'show_visual', tool_args: { title: 'T', kind: 'chart', html_chars: 12, html_start: '<p>' } },
        { tool_name: 'show_visual', tool_args: { visual_id: id, title: 'T', kind: 'chart' } },
        { tool_name: 'show_visual', tool_args: { visual_id: id } },
        { tool_name: 'show_visual', tool_args: { visual_id: '../../etc/passwd' } },
      ]),
    ).toEqual([id]);
    expect(visualsIn(undefined)).toEqual([]);
  });
});
