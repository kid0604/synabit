import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import { isAppOwned, GOVERNED } from '../../../shared/fieldRegistry';
import { kindOf, type FieldKind } from '../../../shared/fieldValue';

/**
 * A type the vault contains, as the vault reports it.
 *
 * Mirrors `ObservedType` in `models/node.rs`.
 */
/** One key, how many nodes of the type carry it, and a taste of what it holds. */
export interface ObservedField {
  key: string;
  count: number;
  /**
   * A small stand-in for the value, for working out the field's kind.
   *
   * Not the value: a list arrives as `[]` and an object as `{}`, and a long
   * string arrives cut short. It is only ever read by `kindOf`, which needs
   * enough to tell a date from a word and nothing more.
   */
  sample?: unknown;
}

export interface ObservedType {
  node_type: string;
  count: number;
  /** Frontmatter keys nodes of this type carry. Not a list of permitted keys. */
  fields: ObservedField[];
}

/**
 * How much of a type has to carry a key before it counts as the type's shape.
 *
 * Half, and the vault it was chosen against makes the case: `task` has eleven
 * keys on 99% of its nodes and then falls off a cliff to 20%, so any threshold
 * between those two picks the same set. `animal` has four keys on half its
 * nodes and `màu` on a quarter — the stray falls out on its own.
 *
 * It is a default, not a rule. A type with three nodes will call almost
 * anything usual, which is noisy but honest: with three nodes there is no
 * habit to find yet.
 */
const USUAL = 0.5;

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
 *
 * `schema` and `view` are this app's own bookkeeping and were missing from the
 * list — so a kind's declared structure turned up in the rail as a kind you
 * could browse, and opening one showed `fields` as a row of raw JSON. The two
 * screens that read those files are the type page and the view bar; nobody
 * opens them as documents, and the moment Things wrote its first schema it was
 * listing its own filing cabinet among the things somebody keeps.
 */
const INTERNAL = new Set([
  'json', 'canvas', 'pdf_highlight', 'pdf_drawing', 'interaction',
  'schema', 'view',
]);

/**
 * Storage rather than something anyone keeps — and so not something anyone
 * manages either.
 *
 * Exported because the screens downstream need to ask. A kind can be opened
 * from the folded section and become the active one, and everything after that
 * used to assume an active kind was a browsable kind: the rail pinned `schema`
 * into the list of what you keep with a count of zero beside it, and the page
 * that manages a kind's structure offered to rename and delete the app's own
 * filing cabinet.
 */
export const isInternalType = (t: string) => INTERNAL.has(t) || t.startsWith('finance_');

const isInternal = isInternalType;

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
    types.value.find(t => t.node_type === nodeType)?.fields.map(f => f.key) ?? [];

  /** Every key on the type with its count, most common first. */
  const observedFor = (nodeType: string): ObservedField[] =>
    types.value.find(t => t.node_type === nodeType)?.fields ?? [];

  /**
   * What kind of value a key holds, read from the vault rather than assumed.
   *
   * Every field used to be recorded as `text` when a shape was first written
   * down, because nothing carried what the values were — so `due_date` on a
   * hundred tasks was declared text, and an empty one drew a text box where a
   * date picker belonged. `kindOf` is still the only thing that decides; it is
   * simply being given something to look at.
   *
   * `null` when there is nothing to look at — a key nobody carries, a vault
   * reported by an older build, or a key that is an empty string everywhere.
   * That is different from text, and the difference is what keeps a warning
   * about a disagreement from firing where there is nothing to disagree with.
   */
  const kindOfField = (nodeType: string, key: string): FieldKind | null => {
    const found = observedFor(nodeType).find(f => f.key === key);
    if (!found || found.sample === undefined) return null;
    // An empty string is not evidence of being text; it is the absence of
    // evidence. `priority` is `''` on every task in this vault, and calling
    // that "the vault says text" would argue with anybody who declared it
    // something else on the strength of nothing at all.
    if (typeof found.sample === 'string' && !found.sample.trim()) return null;
    return kindOf(found.sample);
  };

  /**
   * The keys that make up this type's shape, for offering rather than enforcing.
   *
   * App-owned keys are out: `order` is on every task and belongs to a drag,
   * not to anyone typing. So are `title` and `type`, which are on every node
   * of every kind and are edited as the heading and the chip. What is left is
   * what a person filling in a new one of these would expect to see waiting.
   */
  const usualFieldsFor = (nodeType: string): string[] => {
    const found = types.value.find(t => t.node_type === nodeType);
    if (!found || found.count === 0) return [];
    return found.fields
      .filter(
        f =>
          f.count / found.count >= USUAL &&
          !isAppOwned(nodeType, f.key) &&
          !GOVERNED.has(f.key),
      )
      .map(f => f.key);
  };

  return {
    types, browsable, internal, loading, error, load,
    fieldsFor, observedFor, usualFieldsFor, kindOfField,
  };
}
