/**
 * Recognising the deep link that means "let me write something".
 *
 * Distinct from a capture URL, which carries the text. This one carries
 * nothing: it is what the launcher shortcut fires, and later the desktop
 * hotkey and the tray, all of which mean *open the compose box* rather than
 * *save these words*. `capture_from_url` in Rust deliberately rejects it —
 * there is no text to queue — so the front end handles it.
 *
 * The schemes match the Rust side for the same reason they do there:
 * `com.synabit.app` is what the platforms register, and the shorter spelling
 * is accepted so a change of scheme, or a hand-typed URL, still works.
 */
const SCHEMES = ['com.synabit.app://', 'synabit://'];

export function isComposeUrl(url: string): boolean {
  const scheme = SCHEMES.find((candidate) => url.startsWith(candidate));
  if (!scheme) return false;

  const rest = url.slice(scheme.length);
  const path = (rest.split('?')[0] ?? '').replace(/\/+$/, '');
  return path === 'quickcap/compose';
}
