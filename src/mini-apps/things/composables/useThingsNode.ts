import { ref, computed } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';

/**
 * Keys the app owns, which are never offered as editable rows.
 *
 * Not a judgement about what matters — a judgement about who writes them.
 *
 * `node_id` is the worst one to get wrong. It is the file's identity to the
 * sync engine, recorded so a node keeps its links and its history when it is
 * renamed or moved. Hand-editing one splits the file into two documents on the
 * next sync; clearing one hands it a fresh identity while every other device
 * keeps the old.
 *
 * `title` and `type` are governed but not hidden: the title is edited as the
 * heading, and the type is shown because in a screen that displays every kind
 * of thing, "what kind of thing is this" is worth saying out loud.
 */
const APP_OWNED = new Set(['node_id', 'created_at', 'updated_at', 'timestamp']);

/** Shown, but not as something to type into. */
const READ_ONLY = new Set(['type', 'title']);

export interface FieldRow {
  key: string;
  /** Always text in the row; structure is restored on the way out. */
  value: string;
}

/**
 * A frontmatter value as one line of text.
 *
 * `String(value)` turns a list into `a,b` and an object into `[object Object]`,
 * and saving that back writes the mangling to disk. Anything that is not a
 * scalar is shown as the JSON it is, which round-trips.
 */
function toText(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

/** The inverse. Text that was a list goes back as a list. */
function fromText(text: string): unknown {
  const trimmed = text.trim();
  if (!trimmed.startsWith('[') && !trimmed.startsWith('{')) return text;
  try {
    return JSON.parse(trimmed);
  } catch {
    return text;
  }
}

/**
 * One node, read in full and written back as a patch.
 *
 * The list reads summaries — no bodies, for the reason `NodeSummary` exists —
 * so this is where the whole node is fetched, and it is fetched for one node at
 * a time.
 */
export function useThingsNode() {
  const ns = useNodeService();

  const node = ref<any | null>(null);
  const title = ref('');
  const body = ref('');
  const fields = ref<FieldRow[]>([]);
  const loading = ref(false);
  const saving = ref(false);
  const error = ref<string | null>(null);

  /** What was on disk when this was opened, to work out what changed. */
  let loadedKeys: string[] = [];
  let token = 0;

  const nodeType = computed<string>(() => node.value?.node_type ?? '');

  const readOnlyRows = computed(() =>
    [...READ_ONLY]
      .filter(k => k === 'type' || node.value?.properties?.[k] !== undefined)
      .map(k => ({
        key: k,
        value: k === 'type' ? nodeType.value : toText(node.value?.properties?.[k]),
      })),
  );

  const open = async (id: string) => {
    const mine = ++token;
    loading.value = true;
    error.value = null;
    try {
      const found = await ns.getNode(id);
      if (mine !== token) return;
      if (!found) {
        node.value = null;
        error.value = `Không tìm thấy ${id}`;
        return;
      }
      node.value = found;
      title.value = found.title ?? '';
      body.value = found.content ?? '';

      const props = (found.properties ?? {}) as Record<string, unknown>;
      loadedKeys = Object.keys(props);
      fields.value = loadedKeys
        .filter(k => !APP_OWNED.has(k) && !READ_ONLY.has(k))
        .map(k => ({ key: k, value: toText(props[k]) }));
    } catch (e) {
      if (mine !== token) return;
      logger.error('[Things] Could not open node', e);
      error.value = String(e);
      node.value = null;
    } finally {
      if (mine === token) loading.value = false;
    }
  };

  const close = () => {
    token++;
    node.value = null;
    fields.value = [];
    loadedKeys = [];
    error.value = null;
  };

  const addField = () => fields.value.push({ key: '', value: '' });
  const removeField = (index: number) => fields.value.splice(index, 1);

  /**
   * Write the fields this screen governs, and nothing else.
   *
   * Two halves, and both are the patch contract rather than politeness:
   *
   * - Rows become values. A row the user removed is named as `null`, because a
   *   key merely left out means "I have nothing to say about this one" and
   *   comes straight back on the next read.
   * - App-owned keys are never in the payload at all. Deriving deletions by
   *   subtraction — anything on disk but not in the form — is the trap that
   *   nulls `node_id`, and it is exactly what this loop is written to avoid.
   */
  const save = async () => {
    if (!node.value) return;
    saving.value = true;
    error.value = null;

    try {
      const patch: Record<string, unknown> = {};
      const kept = new Set<string>();

      for (const row of fields.value) {
        const key = row.key.trim();
        if (!key || APP_OWNED.has(key) || READ_ONLY.has(key)) continue;
        kept.add(key);
        patch[key] = fromText(row.value);
      }

      for (const key of loadedKeys) {
        if (APP_OWNED.has(key) || READ_ONLY.has(key)) continue;
        if (!kept.has(key)) patch[key] = null;
      }

      await ns.writeNode({
        relPath: node.value.id,
        // From the node on disk, never a constant. A writer that decides a
        // node's type for itself is how a task opened in the note editor was
        // saved as a note and the task was gone — see `nodeRoutes.ts`.
        nodeType: node.value.node_type,
        title: title.value,
        properties: patch,
        content: body.value,
      });

      loadedKeys = [...kept];
    } catch (e) {
      logger.error('[Things] Could not save node', e);
      error.value = String(e);
    } finally {
      saving.value = false;
    }
  };

  return {
    node, title, body, fields, readOnlyRows, nodeType,
    loading, saving, error,
    open, close, addField, removeField, save,
  };
}
