import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';

/**
 * A type the vault contains, as the vault reports it.
 *
 * Mirrors `ObservedType` in `models/node.rs`.
 */
export interface ObservedType {
  node_type: string;
  count: number;
  /** Frontmatter keys nodes of this type carry. Not a list of permitted keys. */
  fields: string[];
}

/**
 * Types that are storage rather than something anyone browses.
 *
 * Every one of these is real and every one of them would be noise at the top of
 * a list of what you keep. `json` is the worst of them — feed state, message
 * days, whiteboard payloads — and on an ordinary vault it outnumbers notes.
 *
 * They are hidden rather than dropped: `list_observed_types` deliberately
 * returns everything, because "what is in the vault" has one answer and the
 * screen showing it decides what to lead with.
 */
const INTERNAL = new Set(['json', 'canvas', 'pdf_highlight', 'pdf_drawing', 'interaction']);

const isInternal = (t: string) => INTERNAL.has(t) || t.startsWith('finance_');

/**
 * What the vault turns out to hold.
 *
 * The left rail is built from this rather than from a list in the code, which
 * is the whole reason Things can show a type nobody wrote code for: write
 * `type: animal` into a file and Animals appears, with no registration step in
 * between.
 */
export function useObservedTypes() {
  const types = ref<ObservedType[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const load = async () => {
    loading.value = true;
    error.value = null;
    try {
      types.value = await invoke<ObservedType[]>('list_observed_types');
    } catch (e) {
      logger.error('[Things] Could not read the vault’s types', e);
      error.value = String(e);
      types.value = [];
    } finally {
      loading.value = false;
    }
  };

  /**
   * What to lead with: the types a person put things into.
   *
   * `finance_*` is excluded for a second reason beyond noise — `run_node_query`
   * refuses to return those rows, so offering one in the rail would open an
   * empty list and look like a bug.
   */
  const browsable = computed(() => types.value.filter(t => !isInternal(t.node_type)));

  /** The rest, for a section that stays collapsed until someone asks. */
  const internal = computed(() => types.value.filter(t => isInternal(t.node_type)));

  /** Fields this type actually uses, for the arrangement menus. */
  const fieldsFor = (nodeType: string): string[] =>
    types.value.find(t => t.node_type === nodeType)?.fields ?? [];

  return { types, browsable, internal, loading, error, load, fieldsFor };
}
