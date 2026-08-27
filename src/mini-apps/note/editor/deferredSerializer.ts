/**
 * Turning the document into markdown, but not on every keystroke.
 *
 * Serialising walks the whole document and the result is scanned again by
 * regex, so the cost grows with the length of the note while the thing that
 * triggers it — one character — does not. On a long note that is felt directly
 * as the editor lagging behind the keyboard.
 *
 * Waiting costs nothing, because nothing needs the markdown the instant it is
 * typed: the autosave that consumes it is on a timer of its own, and the
 * person is still writing. What does need it now — a save from elsewhere, an
 * export, a tab closing — calls `flush`.
 */
export interface DeferredSerializer {
  /** Something changed; produce the markdown shortly. */
  schedule(): void;
  /** Produce it now, if anything is waiting. */
  flush(): void;
  /** Stop waiting, and produce nothing. */
  cancel(): void;
  /**
   * Whether a value arriving from outside is simply our own coming back.
   *
   * The parent stores what we emit and hands it straight back, and answering
   * that by serialising the document all over again just to compare meant
   * every keystroke paid for two full walks of it — one of which could only
   * ever conclude that nothing had changed.
   */
  isEcho(value: string): boolean;
  /** Record a value that came from outside as the current one. */
  adopt(value: string): void;
}

export function createDeferredSerializer(opts: {
  /** Walk the document. `null` when there is nothing to walk. */
  produce: () => string | null;
  emit: (value: string) => void;
  delayMs: number;
  setTimer?: typeof setTimeout;
  clearTimer?: typeof clearTimeout;
}): DeferredSerializer {
  const setTimer = opts.setTimer ?? setTimeout;
  const clearTimer = opts.clearTimer ?? clearTimeout;

  let timer: ReturnType<typeof setTimeout> | undefined;
  let lastEmitted: string | null = null;

  const run = () => {
    timer = undefined;
    const produced = opts.produce();
    if (produced === null) return;
    // Identical output is not worth waking anything downstream for: it would
    // re-arm the autosave and rewrite a file with the bytes already in it.
    if (produced === lastEmitted) return;
    lastEmitted = produced;
    opts.emit(produced);
  };

  return {
    schedule() {
      if (timer !== undefined) clearTimer(timer);
      timer = setTimer(run, opts.delayMs);
    },
    flush() {
      if (timer === undefined) return;
      clearTimer(timer);
      run();
    },
    cancel() {
      if (timer === undefined) return;
      clearTimer(timer);
      timer = undefined;
    },
    isEcho: (value: string) => value === lastEmitted,
    adopt(value: string) {
      lastEmitted = value;
    },
  };
}
