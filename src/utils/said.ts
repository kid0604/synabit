/**
 * What the backend said, in words.
 *
 * # Why this is not `String(e)`
 *
 * `AppError` serialises to `{ code, message }` — see `src-tauri/src/error.rs`,
 * which builds that object on purpose so a caller can branch on the code. What
 * a Tauri command rejects with is therefore an **object**, and `String()` of an
 * object is `"[object Object]"`.
 *
 * That is what every refusal this app writes looked like on screen. The query
 * engine refuses rather than quietly answering a different question — an
 * unreadable date, a keyword that was renamed, a question that would spend
 * money and how much — and each of those sentences arrived as `[object
 * Object]`. The engine was careful and the screen threw it away.
 *
 * The Messages app had already hit this and written the same two lines four
 * times in four composables. This is that, once.
 */
export function said(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  const message = (error as { message?: unknown } | null | undefined)?.message;
  if (typeof message === 'string' && message.trim()) return message;
  // Nothing readable in it. Better the raw shape than an empty box, because an
  // empty box reads as "nothing went wrong".
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}
