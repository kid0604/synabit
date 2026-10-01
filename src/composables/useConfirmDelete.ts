import { ref } from 'vue';
import { useAppStore } from '../stores/useAppStore';

/**
 * The one question a delete may ask: "Delete this?".
 *
 * Every delete in the app calls `confirmDelete` before it hides anything. With
 * "Ask before deleting" off — the default — it answers yes at once and the
 * undo toast is the way back; with it on, it opens the shared dialog
 * (`DeleteConfirmHost`, mounted once in App.vue) and resolves with the answer.
 * One function, so the setting means the same thing in every app.
 *
 * Deletes that ask a question of their own — a task with subtasks (which
 * ones?), emptying the trash (it is final) — keep asking it; this is only the
 * yes/no in front of an ordinary, undoable delete.
 */
export interface DeleteQuestion {
  /** What is being deleted, as the user knows it; omitted for "Delete this?". */
  name?: string;
  /** How many, for a bulk delete. */
  count?: number;
  /** False when the thing does not go to the trash — the dialog then says undo is the only way back. */
  toTrash?: boolean;
}

interface Pending extends DeleteQuestion {
  resolve: (yes: boolean) => void;
}

export const pendingDeleteQuestion = ref<Pending | null>(null);

export function confirmDelete(question: DeleteQuestion = {}): Promise<boolean> {
  if (!useAppStore().confirmBeforeDelete) return Promise.resolve(true);
  // A second question while one is open answers the first with no.
  pendingDeleteQuestion.value?.resolve(false);
  return new Promise((resolve) => {
    pendingDeleteQuestion.value = { toTrash: true, ...question, resolve };
  });
}

export function answerDeleteQuestion(yes: boolean) {
  const p = pendingDeleteQuestion.value;
  pendingDeleteQuestion.value = null;
  p?.resolve(yes);
}
