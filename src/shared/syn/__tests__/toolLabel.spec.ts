import { describe, it, expect } from 'vitest';
import tools from '../../../../src-tauri/src/syn/tools.rs?raw';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import { LABELLED_TOOLS, connectorTool, shortCount, toolLabel } from '../toolLabel';

/**
 * Every tool the model can call has words a person reads.
 *
 * The list of declarations is read out of `tools.rs` itself, so a tool added
 * there without a sentence here fails this test rather than showing its raw
 * name on screen — which is what the whole screen showed before.
 */
const NAMED_BY_CONSTANT: Record<string, string> = {
  BROWSE_TOOL: 'browse',
  LOOK_BACK_TOOL: 'look_back',
  CAPTURE_TOOL: 'capture',
  PLAN_TOOL: 'update_plan',
  LOAD_TOOL: 'load_skill',
  RUN_TOOL: 'run_recipe',
  // `crate::syn::delegate::TOOL`, whose module names it.
  TOOL: 'delegate',
  FIND_TOOL: 'find_tools',
};

const declared = (): string[] => {
  const names = new Set<string>();
  for (const [, literal] of tools.matchAll(/name: "([a-z_]+)"\.to_string\(\)/g)) names.add(literal);
  for (const [, constant] of tools.matchAll(/name: (?:crate::syn::\w+::)?([A-Z_]+)\.to_string\(\)/g)) {
    const name = NAMED_BY_CONSTANT[constant];
    expect(name, `${constant} is declared as a tool; add what it is called`).toBeDefined();
    names.add(name);
  }
  return [...names];
};

describe('what a tool is doing, in words', () => {
  it('has a sentence for every tool the model can call', () => {
    const names = declared();
    expect(names.length).toBeGreaterThan(30);
    for (const name of names) {
      expect(LABELLED_TOOLS, `${name} has no label`).toContain(name);
    }
  });

  it('says it in both languages', () => {
    for (const name of LABELLED_TOOLS) {
      expect(en.syn, name).toHaveProperty(`doing_${name}`);
      expect(vi.syn, name).toHaveProperty(`doing_${name}`);
    }
  });

  it('still says something true about a tool it has not heard of', () => {
    const t = (key: string, values?: Record<string, unknown>) => `${key}:${JSON.stringify(values ?? {})}`;
    expect(toolLabel(t, 'query_nodes')).toBe('syn.doing_query_nodes:{}');
    expect(toolLabel(t, 'mcp_jira_search')).toBe('syn.doing_other:{"tool":"mcp_jira_search"}');
  });

  it('says which connector a tool is being used on', () => {
    const t = (key: string, values?: Record<string, unknown>) => `${key}:${JSON.stringify(values ?? {})}`;
    expect(toolLabel(t, 'connector__jira__search_issues')).toBe(
      'syn.doing_connector:{"server":"jira","tool":"search_issues"}',
    );
    expect(connectorTool('connector__files__read__all')).toEqual({ server: 'files', tool: 'read__all' });
    expect(connectorTool('connector__jira__')).toBeNull();
    expect(connectorTool('connector____x')).toBeNull();
    // Runs from before connectors had their name still read.
    expect(connectorTool('mcp__jira__search_issues')).toEqual({ server: 'jira', tool: 'search_issues' });
    expect(en.syn).toHaveProperty('doing_connector');
    expect(vi.syn).toHaveProperty('doing_connector');
    expect(en.syn.doing_connector).toContain('{server}');
    expect(vi.syn.doing_connector).toContain('{server}');
  });

  it('keeps a count short enough for one line', () => {
    expect(shortCount(950)).toBe('950');
    expect(shortCount(12_400)).toBe('12k');
    expect(shortCount(1_250_000)).toBe('1.3M');
  });
});
