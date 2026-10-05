/**
 * Work to finish before the app quits.
 *
 * Quitting used to end the process at once. Anything still waiting to be
 * written — a board's autosave runs two seconds after the last change — was
 * lost if the user quit inside that window. Now the Rust side holds the quit,
 * asks the front end to finish (`app:before-quit`), and quits when it is told
 * the work is done, or after a short wait if it never is.
 *
 * An app registers what it needs done; `runBeforeQuit` runs every task at
 * once and waits for them, never longer than `limitMs`.
 */
type Task = () => Promise<unknown> | unknown;

const tasks = new Set<Task>();

/** Run `task` before the app quits. Returns the function that unregisters it. */
export function onBeforeQuit(task: Task): () => void {
  tasks.add(task);
  return () => tasks.delete(task);
}

/** Every registered task, together, for at most `limitMs`. A task that fails does not hold up the rest. */
export async function runBeforeQuit(limitMs = 2500): Promise<void> {
  const all = Promise.allSettled([...tasks].map(async (task) => task()));
  await Promise.race([all, new Promise((resolve) => setTimeout(resolve, limitMs))]);
}
