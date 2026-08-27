import { invoke } from '@tauri-apps/api/core';

import { useNodeService } from '../../composables/useNodeService';
import { logger } from '../../utils/logger';
import { deriveTitle, extractTags } from './parsing';

/**
 * The one way a quick cap gets written.
 *
 * Extracted from `QuickCapApp.vue` when captures started arriving from
 * outside the app — a share sheet, a widget, a hotkey. Those have no
 * business knowing about frontmatter, and a second implementation of "turn
 * text into a cap" would drift from this one: a shared cap would end up
 * with different tags, or a title derived by an older rule, and the
 * difference would only surface as search quietly missing things.
 */
export function useQuickCapWriter() {
  const ns = useNodeService();

  /**
   * Write a cap, deriving everything that is derived.
   *
   * `properties.tags` is what the tag manager, `rename_tag` and the FTS
   * `tags` column all read; the body is what the user actually types.
   * Deriving one from the other on every single write is the only way they
   * cannot drift — which is exactly how they drifted far enough to need a
   * migration.
   */
  async function writeCap(params: {
    relPath: string;
    nodeType: string;
    properties: Record<string, unknown>;
    content: string;
    eventType?: 'created' | 'updated';
  }) {
    await ns.writeNode({
      relPath: params.relPath,
      nodeType: params.nodeType as never,
      title: deriveTitle(params.content),
      properties: { ...params.properties, tags: extractTags(params.content) },
      content: params.content,
      eventType: params.eventType,
    });
  }

  /** Create a new cap from text. Returns its path. */
  async function createCap(content: string, source?: string): Promise<string> {
    const relPath = `QuickCaps/${crypto.randomUUID()}.md`;
    await writeCap({
      relPath,
      nodeType: 'quickcap',
      content,
      // Recorded so the app can eventually say where a week's caps came
      // from, and so a misbehaving surface can be identified rather than
      // guessed at. Absent for anything typed into the app itself.
      properties: source ? { source } : {},
      eventType: 'created',
    });
    return relPath;
  }

  return { writeCap, createCap };
}

interface QueuedCapture {
  id: string;
  text: string;
  source: string | null;
  received_at: string;
}

/**
 * Write down everything that arrived while no vault was open.
 *
 * Captures are queued rather than written, because they arrive whenever the
 * user has the thought — which is regularly a moment when the vault is
 * locked or not yet loaded. See `src-tauri/src/commands/capture.rs`.
 */
export function useCaptureIntake() {
  const { createCap } = useQuickCapWriter();

  /** Returns how many captures became caps. */
  async function drainCaptures(): Promise<number> {
    // Captures taken while the app was closed live as files written by the
    // no-window Android activity; move them into the real queue first so
    // everything below sees one ordered list.
    try {
      await invoke<number>('import_handoff_captures');
    } catch (e) {
      logger.error('Could not import handed-off captures', e);
    }

    let queue: QueuedCapture[];
    try {
      queue = await invoke<QueuedCapture[]>('list_queued_captures');
    } catch (e) {
      logger.error('Could not read the capture queue', e);
      return 0;
    }
    if (queue.length === 0) return 0;

    let written = 0;
    for (const capture of queue) {
      try {
        await createCap(capture.text, capture.source ?? undefined);
        // Dropped only once the cap is on disk. An interruption between the
        // two leaves a duplicate, which is recoverable in a way that a lost
        // thought is not.
        await invoke('drop_queued_capture', { id: capture.id });
        written += 1;
      } catch (e) {
        // Stop rather than skip: the queue is in arrival order, and carrying
        // on past a failure would write later captures before earlier ones.
        logger.error(`Could not write queued capture ${capture.id}`, e);
        break;
      }
    }

    if (written > 0) {
      logger.info(`Wrote ${written} queued capture(s) into the vault`);
    }
    return written;
  }

  return { drainCaptures };
}
