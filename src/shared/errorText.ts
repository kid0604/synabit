/**
 * What an error from a Tauri command says, in words.
 *
 * A Rust `AppError` reaches the frontend as `{ code, message }`, so `String(e)`
 * shows "[object Object]" — and any check on its text never matches.
 */
export function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error && typeof error === 'object' && 'message' in error && typeof (error as { message: unknown }).message === 'string') {
    return (error as { message: string }).message;
  }
  return String(error);
}
