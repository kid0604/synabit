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
  'search_feed_articles', 'read_feed_article', 'update_feed_article', 'search_files',
  'read_file_text', 'get_finance_summary', 'search_finance', 'get_transactions',
  'create_transaction', 'remember', 'recall', 'load_skill', 'run_recipe', 'look_back', 'browse',
  'timeline', 'read_board', 'draw_board', 'edit_board', 'capture', 'update_plan', 'rename_field',
  'delete_field', 'rename_kind', 'delete_kind', 'delegate',
] as const;

export function toolLabel(t: (key: string, values?: Record<string, unknown>) => string, tool: string): string {
  return (LABELLED_TOOLS as readonly string[]).includes(tool)
    ? t(`syn.doing_${tool}`)
    : t('syn.doing_other', { tool });
}

/** Tokens, short enough for one line: 950, 12k, 1.2M. */
export function shortCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}
