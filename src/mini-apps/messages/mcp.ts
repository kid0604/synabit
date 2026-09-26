/**
 * Syn's MCP servers, as the settings screen reads and writes them.
 *
 * The shapes mirror `syn::mcp` in Rust. What is here and not there is the
 * form: a server is edited as one flat draft — a name, an address or a
 * program, and a list of secret name/value pairs — and turned back into the
 * shape Rust stores, with the values split off to go to the keychain.
 */

export type McpTransport =
  | { kind: 'http'; url: string; secret_headers: string[] }
  | { kind: 'stdio'; command: string; args: string[]; env_keys: string[] };

export interface McpServer {
  id: string;
  name: string;
  transport: McpTransport;
  enabled: boolean;
}

export type McpStatus =
  | { state: 'connected' }
  | { state: 'failed'; reason: string }
  | { state: 'desktop_only' }
  | { state: 'not_trusted_here' }
  | { state: 'off' };

export interface McpToolView {
  name: string;
  description: string;
  read_only: boolean;
}

export interface McpServerView {
  server: McpServer;
  /** `null` until the server has been tried since the app opened. */
  status: McpStatus | null;
  tools: McpToolView[];
  /** Names of the secrets this device holds for it. Never the values. */
  secrets_here: string[];
}

export interface McpTested {
  ok: boolean;
  desktop_only: boolean;
  error: string | null;
  tools: McpToolView[];
}

export interface McpSecretDraft {
  name: string;
  /** Typed now. Empty keeps what the keychain already holds. */
  value: string;
  /** Whether this device already holds a value for it. */
  stored: boolean;
}

export interface McpDraft {
  id: string;
  name: string;
  kind: 'http' | 'stdio';
  url: string;
  command: string;
  /** One argument per line, so an argument with a space in it stays one. */
  args: string;
  secrets: McpSecretDraft[];
  enabled: boolean;
}

/** A form for a new server, or for editing one. */
export function draftFrom(view?: McpServerView): McpDraft {
  if (!view) {
    return { id: '', name: '', kind: 'http', url: '', command: '', args: '', secrets: [], enabled: true };
  }
  const t = view.server.transport;
  const names = t.kind === 'http' ? t.secret_headers : t.env_keys;
  return {
    id: view.server.id,
    name: view.server.name,
    kind: t.kind,
    url: t.kind === 'http' ? t.url : '',
    command: t.kind === 'stdio' ? t.command : '',
    args: t.kind === 'stdio' ? t.args.join('\n') : '',
    secrets: names.map(name => ({ name, value: '', stored: view.secrets_here.includes(name) })),
    enabled: view.server.enabled,
  };
}

/**
 * What to send to `syn_mcp_save` / `syn_mcp_test`: the server as stored, and
 * the secret values typed just now, by name. A blank name is dropped; a blank
 * value is left out, which keeps what the keychain holds.
 */
export function serverFrom(draft: McpDraft): { server: McpServer; secrets: Record<string, string> } {
  const named = draft.secrets.filter(s => s.name.trim() !== '');
  const names = named.map(s => s.name.trim());
  const secrets: Record<string, string> = {};
  for (const s of named) {
    if (s.value.trim() !== '') secrets[s.name.trim()] = s.value;
  }
  const transport: McpTransport =
    draft.kind === 'http'
      ? { kind: 'http', url: draft.url.trim(), secret_headers: names }
      : {
          kind: 'stdio',
          command: draft.command.trim(),
          args: draft.args.split('\n').map(a => a.trim()).filter(a => a !== ''),
          env_keys: names,
        };
  return { server: { id: draft.id, name: draft.name.trim(), transport, enabled: draft.enabled }, secrets };
}

type T = (key: string, values?: Record<string, unknown>) => string;

/** One dot and one sentence for where a server stands. */
export function statusLine(t: T, view: McpServerView): { dot: string; text: string } {
  const s = view.status;
  const off = { dot: 'bg-gray-300 dark:bg-gray-600', text: t('syn.mcp_status_off') };
  if (!view.server.enabled) return off;
  if (!s) return { dot: 'bg-gray-300 dark:bg-gray-600', text: t('syn.mcp_status_pending') };
  switch (s.state) {
    case 'off':
      return off;
    case 'connected':
      return { dot: 'bg-green-500', text: t('syn.mcp_status_connected', { n: view.tools.length }) };
    case 'failed':
      return { dot: 'bg-red-500', text: t('syn.mcp_status_failed', { reason: s.reason }) };
    case 'desktop_only':
      return { dot: 'bg-gray-300 dark:bg-gray-600', text: t('syn.mcp_status_desktop_only') };
    case 'not_trusted_here':
      return { dot: 'bg-amber-500', text: t('syn.mcp_status_not_trusted_here') };
  }
}
