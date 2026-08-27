import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createDeferredSerializer } from '../deferredSerializer';

const make = (produce: () => string | null, delayMs = 200) => {
  const emit = vi.fn();
  const producer = vi.fn(produce);
  const s = createDeferredSerializer({ produce: producer, emit, delayMs });
  return { s, emit, producer };
};

describe('createDeferredSerializer', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('walks the document once for a burst of typing, not once per keystroke', () => {
    // The whole point. Serialising costs time proportional to the note; the
    // trigger is one character. Ten characters should not cost ten walks.
    let doc = '';
    const { s, producer, emit } = make(() => doc);

    for (const ch of 'xin chào') {
      doc += ch;
      s.schedule();
      vi.advanceTimersByTime(10);
    }
    vi.advanceTimersByTime(500);

    expect(producer).toHaveBeenCalledTimes(1);
    expect(emit).toHaveBeenCalledExactlyOnceWith('xin chào');
  });

  it('produces immediately when flushed, so nothing typed is left behind', () => {
    // A save arriving from a rename, an export, a tab closing.
    const doc = 'nửa câu';
    const { s, emit } = make(() => doc);

    s.schedule();
    s.flush();

    expect(emit).toHaveBeenCalledExactlyOnceWith('nửa câu');
  });

  it('does nothing when flushed with nothing waiting', () => {
    const { s, producer } = make(() => 'anything');

    s.flush();

    expect(producer).not.toHaveBeenCalled();
  });

  it('stays quiet when the document comes out the same', () => {
    // An identical emit would re-arm the autosave and rewrite a file with the
    // bytes already in it.
    const { s, emit } = make(() => 'unchanged');

    s.schedule();
    vi.advanceTimersByTime(500);
    s.schedule();
    vi.advanceTimersByTime(500);

    expect(emit).toHaveBeenCalledTimes(1);
  });

  it('cancels without producing anything', () => {
    const { s, emit, producer } = make(() => 'gone');

    s.schedule();
    s.cancel();
    vi.advanceTimersByTime(500);

    expect(producer).not.toHaveBeenCalled();
    expect(emit).not.toHaveBeenCalled();
  });

  it('recognises its own value coming back from the parent', () => {
    // The parent stores what we emit and hands it straight back. Answering
    // that by serialising all over again is the second walk per keystroke
    // this exists to remove.
    const { s } = make(() => 'round trip');

    expect(s.isEcho('round trip')).toBe(false);
    s.schedule();
    vi.advanceTimersByTime(500);

    expect(s.isEcho('round trip')).toBe(true);
    expect(s.isEcho('something else')).toBe(false);
  });

  it('adopts a value that arrived from outside', () => {
    // A version restore writes over the editor. What it wrote is now the
    // current text, and must not read as a change to send back.
    const { s } = make(() => 'whatever');

    s.adopt('restored from history');

    expect(s.isEcho('restored from history')).toBe(true);
  });

  it('emits nothing when there is no document to walk', () => {
    const { s, emit } = make(() => null);

    s.schedule();
    vi.advanceTimersByTime(500);

    expect(emit).not.toHaveBeenCalled();
  });
});
