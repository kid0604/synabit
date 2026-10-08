import { logger } from '../../utils/logger';

/**
 * A feed error in words a person can act on.
 *
 * The backend's messages are written for the log — "Not a usable URL:
 * relative URL without a base", "HTTP 403 from https://…" — and the Add feed
 * dialog used to show them as they were. Each known kind maps to a sentence
 * that says what went wrong and what to try; the raw text goes to the log.
 * Returns the i18n key (plus params) so callers translate it themselves.
 */
export function feedErrorKey(e: unknown, fallback: string): { key: string; params?: Record<string, string> } {
  const raw = typeof e === 'string' ? e : (e as { message?: string } | null)?.message ?? '';
  logger.warn('[Feeds]', raw || e);
  if (/Not a usable URL|URL has no host/i.test(raw)) return { key: 'feeds.error_url_invalid' };
  if (/Refusing to fetch|private network|on this machine/i.test(raw)) return { key: 'feeds.error_url_private' };
  const status = raw.match(/HTTP (\d{3})/);
  if (status) {
    return status[1] === '404'
      ? { key: 'feeds.error_not_found' }
      : { key: 'feeds.error_site_refused', params: { status: status[1] } };
  }
  if (/too large/i.test(raw)) return { key: 'feeds.error_too_large' };
  if (/timed? ?out|dns|resolve|connect|network|error sending request|tls|certificate/i.test(raw)) {
    return { key: 'feeds.error_unreachable' };
  }
  return { key: fallback };
}
