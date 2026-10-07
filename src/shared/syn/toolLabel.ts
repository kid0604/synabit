/**
 * What a tool is doing, in words a person reads.
 *
 * The screen used to print the tool's own name — `query_nodes`, `get_node` —
 * which is the model's vocabulary and nobody else's. A run of six rounds with
 * nothing but those on screen is a run nobody can follow, so each tool has a
 * sentence in both languages, and one this list has not heard of still says
 * something true: that a tool of that name is being used.
 */
export const LABELLED_TOOLS = [
  'query_nodes', 'get_node', 'list_schemas', 'get_linked_nodes', 'create_node', 'update_node',
  'trash_node', 'restore_node', 'list_trash', 'list_versions', 'restore_version',
  'safe_list', 'safe_health', 'safe_request',
  'search_feed_articles', 'read_feed_article', 'update_feed_article', 'search_files',
  'read_file_text', 'get_finance_summary', 'search_finance', 'get_transactions',
  'create_transaction', 'update_transaction', 'delete_transaction', 'read_spreadsheet',
  'write_spreadsheet', 'remember', 'recall', 'load_skill', 'run_recipe', 'look_back', 'browse',
  'timeline', 'read_board', 'draw_board', 'edit_board', 'capture', 'update_plan', 'rename_field',
  'delete_field', 'rename_kind', 'delete_kind', 'delegate', 'find_tools', 'table_rows',
] as const;

export function toolLabel(t: (key: string, values?: Record<string, unknown>) => string, tool: string): string {
  if ((LABELLED_TOOLS as readonly string[]).includes(tool)) return t(`syn.doing_${tool}`);
  const onConnector = connectorTool(tool);
  if (onConnector) return t('syn.doing_connector', onConnector);
  return t('syn.doing_other', { tool });
}

/**
 * `connector__jira__search_issues` → `{ server: 'jira', tool: 'search_issues' }`.
 *
 * A tool on a connector is named `connector__<connector>__<tool>` by
 * `syn::connector`, and the connector part never holds `__`, so the first one
 * after the prefix is where the tool begins. These come and go with the
 * connectors a person adds, so they cannot be in the list above; saying which
 * connector is what a person watching needs, because it is the part that has
 * left the computer.
 *
 * `mcp__` is the prefix these had before connectors had their name, and it is
 * still in the transcripts of runs from then.
 */
const PREFIXES = ['connector__', 'mcp__'];

export function connectorTool(name: string): { server: string; tool: string } | null {
  const prefix = PREFIXES.find(p => name.startsWith(p));
  if (!prefix) return null;
  const rest = name.slice(prefix.length);
  const cut = rest.indexOf('__');
  if (cut <= 0 || cut + 2 >= rest.length) return null;
  return { server: rest.slice(0, cut), tool: rest.slice(cut + 2) };
}

/** Tokens, short enough for one line: 950, 12k, 1.2M. */
export function shortCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}
