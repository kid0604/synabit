import { describe, it, expect } from 'vitest';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import { draftFrom, serverFrom, statusLine, type McpServerView } from '../mcp';

const t = (key: string, values?: Record<string, unknown>) => `${key}:${JSON.stringify(values ?? {})}`;

const jira: McpServerView = {
  server: {
    id: 'srv-1',
    name: 'Jira',
    transport: { kind: 'http', url: 'https://mcp.example/jira', secret_headers: ['Authorization'] },
    enabled: true,
  },
  status: { state: 'connected' },
  tools: [
    { name: 'search', description: '', read_only: true },
    { name: 'create_issue', description: '', read_only: false },
  ],
  secrets_here: ['Authorization'],
};

describe('an MCP server, as the settings form edits it', () => {
  it('shows which secrets this device holds, and never a value', () => {
    const draft = draftFrom(jira);
    expect(draft.secrets).toEqual([{ name: 'Authorization', value: '', stored: true }]);
  });

  it('sends a secret only when one was typed, so an untouched field keeps the stored one', () => {
    const draft = draftFrom(jira);
    expect(serverFrom(draft).secrets).toEqual({});
    draft.secrets[0].value = 'Bearer new';
    const { server, secrets } = serverFrom(draft);
    expect(secrets).toEqual({ Authorization: 'Bearer new' });
    expect(server.transport).toEqual({ kind: 'http', url: 'https://mcp.example/jira', secret_headers: ['Authorization'] });
    expect(JSON.stringify(server)).not.toContain('Bearer');
  });

  it('keeps an argument with a space in it as one argument', () => {
    const draft = draftFrom();
    draft.kind = 'stdio';
    draft.name = 'Files';
    draft.command = ' /usr/local/bin/files-mcp ';
    draft.args = '--root\n/Users/me/My Documents\n\n';
    draft.secrets = [{ name: 'FILES_TOKEN', value: 't', stored: false }, { name: ' ', value: 'x', stored: false }];
    const { server, secrets } = serverFrom(draft);
    expect(server.transport).toEqual({
      kind: 'stdio',
      command: '/usr/local/bin/files-mcp',
      args: ['--root', '/Users/me/My Documents'],
      env_keys: ['FILES_TOKEN'],
    });
    expect(secrets).toEqual({ FILES_TOKEN: 't' });
  });

  it('says where each server stands, in words that exist in both languages', () => {
    const lines = [
      statusLine(t, jira),
      statusLine(t, { ...jira, status: { state: 'failed', reason: 'refused' } }),
      statusLine(t, { ...jira, status: { state: 'desktop_only' } }),
      statusLine(t, { ...jira, status: { state: 'not_trusted_here' } }),
      statusLine(t, { ...jira, status: null }),
      statusLine(t, { ...jira, server: { ...jira.server, enabled: false } }),
    ];
    expect(lines[0].text).toBe('syn.mcp_status_connected:{"n":2}');
    expect(lines[1].text).toContain('refused');
    expect(lines[2].text).toBe('syn.mcp_status_desktop_only:{}');
    for (const { text } of lines) {
      const key = text.split(':')[0].replace('syn.', '');
      expect(en.syn, key).toHaveProperty(key);
      expect(vi.syn, key).toHaveProperty(key);
    }
  });

  it('has every MCP sentence in both languages', () => {
    const enKeys = Object.keys(en.syn).filter(k => k.startsWith('mcp_'));
    const viKeys = Object.keys(vi.syn).filter(k => k.startsWith('mcp_'));
    expect(enKeys.length).toBeGreaterThan(20);
    expect(viKeys.sort()).toEqual(enKeys.sort());
    expect(en.syn.mcp_honest).toBe(
      'Tools from this server see what Syn sends them; results are treated as untrusted.',
    );
  });
});
