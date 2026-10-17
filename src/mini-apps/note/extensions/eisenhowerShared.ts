import type { Node as PMNode } from '@tiptap/pm/model';

/** What a task chip shows of the task it stands for. */
export interface MatrixTask {
  id: string;
  title: string;
  status: string;
  due_date: string;
  /** Where the Tasks Matrix has it — see `taskQuadrant`. */
  quadrant: string;
}

/**
 * How the matrix reaches the vault. Supplied by the editor that mounts it;
 * without them the matrix is text and links, with nothing to create or tick.
 */
export interface EisenhowerOptions {
  /** The task, `null` when it no longer exists. */
  loadTask?: (id: string) => Promise<MatrixTask | null>;
  toggleTask?: (id: string) => Promise<void>;
  /** A new task in `quadrant`, its id. */
  createTask?: (title: string, quadrant: string) => Promise<string>;
  openTask?: (id: string) => void;
  onConvertFailed?: (error: unknown) => void;
  /** The whole matrix was just removed, by its own button. */
  onDeleted?: () => void;
}

/** The quadrant a position sits in, or `null` outside any matrix. */
export function quadrantAt(doc: PMNode, pos: number): string | null {
  const $pos = doc.resolve(Math.max(0, Math.min(pos, doc.content.size)));
  for (let d = $pos.depth; d > 0; d--) {
    const n = $pos.node(d);
    if (n.type.name === 'eisenhowerCell') return n.attrs.quadrant;
  }
  return null;
}
