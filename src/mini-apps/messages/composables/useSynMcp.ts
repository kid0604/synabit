import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import { serverFrom, type McpDraft, type McpServerView, type McpTested } from '../mcp';

const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

/**
 * Syn's MCP servers, as the settings screen drives them.
 *
 * Every command answers with the whole list afterwards, so the screen draws
 * what Rust found — connected, failed, needs the desktop app — rather than
 * what it expected to happen.
 */
export function useSynMcp(vaultPath: () => string) {
  const servers = ref<McpServerView[]>([]);
  const busy = ref(false);
  const error = ref<string | null>(null);

  const run = async <T>(what: string, call: () => Promise<T>): Promise<T | null> => {
    busy.value = true;
    error.value = null;
    try {
      return await call();
    } catch (e) {
      logger.error(`[Syn] MCP: could not ${what}`, e);
      error.value = asMessage(e);
      return null;
    } finally {
      busy.value = false;
    }
  };

  const take = (views: McpServerView[] | null) => {
    if (views) servers.value = views;
    return views !== null;
  };

  const load = async () =>
    take(await run('list servers', () => invoke<McpServerView[]>('syn_mcp_list', { vaultPath: vaultPath() })));

  const reconnect = async () =>
    take(await run('reconnect', () => invoke<McpServerView[]>('syn_mcp_reconnect', { vaultPath: vaultPath() })));

  const save = async (draft: McpDraft) => {
    const { server, secrets } = serverFrom(draft);
    return take(
      await run('save a server', () =>
        invoke<McpServerView[]>('syn_mcp_save', { vaultPath: vaultPath(), server, secrets }),
      ),
    );
  };

  const remove = async (serverId: string) =>
    take(
      await run('remove a server', () =>
        invoke<McpServerView[]>('syn_mcp_delete', { vaultPath: vaultPath(), serverId }),
      ),
    );

  const test = async (draft: McpDraft) => {
    const { server, secrets } = serverFrom(draft);
    return run('test a server', () =>
      invoke<McpTested>('syn_mcp_test', { vaultPath: vaultPath(), server, secrets }),
    );
  };

  return { servers, busy, error, load, reconnect, save, remove, test };
}
