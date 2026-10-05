/**
 * A link on a board item: a web page, an email address, or something in the
 * vault (`synabit://<kind>/<id>`). Anything else — `javascript:`, `file:`, a
 * custom scheme — is not taken: a link on a board is followed with one click,
 * and a board can arrive from elsewhere.
 */
export function cleanLink(raw: string): string | null {
  const text = raw.trim();
  if (!text) return null;
  if (/^synabit:\/\/[^/]+\/.+/.test(text)) return text;
  // "example.com/page": a web address without its scheme.
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(text) ? text : `https://${text}`;
  try {
    const url = new URL(withScheme);
    if (url.protocol === 'http:' || url.protocol === 'https:') return url.href;
    if (url.protocol === 'mailto:' && url.pathname.includes('@')) return url.href;
  } catch {
    return null;
  }
  return null;
}

/** A vault link's kind and id, or null for a link out of the vault. */
export function vaultTarget(link: string): { kind: string; id: string } | null {
  const m = link.match(/^synabit:\/\/([^/]+)\/(.+)$/);
  if (!m) return null;
  // A `%` that starts no escape ("50%-off.md") is taken as written, not
  // thrown on: a link from a board made elsewhere is not to be trusted to be
  // well formed.
  let id = m[2];
  try {
    id = decodeURIComponent(m[2]);
  } catch {
    // kept as written
  }
  return { kind: m[1], id };
}
