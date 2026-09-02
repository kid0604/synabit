import { ref, computed } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';
import { isAppOwned, appOwnedKeys, GOVERNED } from '../../../shared/fieldRegistry';
import { folderForType } from '../../../shared/nodeRoutes';
import { kindOf, toText, valueOf, asFieldKind, type FieldKind } from '../../../shared/fieldValue';

/**
 * A field the kind is shaped to hold, and how to draw it while it is empty.
 *
 * The kind matters only until there is a value: `kindOf` reads it from the
 * value after that, because a declaration that disagrees with the file is
 * wrong about the file. It is what turns an empty `vaccinated_at` into a date
 * picker rather than a text box that happens to want a date.
 */
export interface Shaped {
  key: string;
  kind: FieldKind;
}

export interface FieldRow {
  key: string;
  /** Always text in the row; the original is what makes writing it back safe. */
  value: string;
  /** How to draw it: a switch, a date, chips, a number, a line of text. */
  kind: FieldKind;
  /**
   * Exactly what was read from disk for this key.
   *
   * Kept so an untouched field is written back unchanged rather than
   * re-derived from its own display text — see `valueOf`.
   */
  original: unknown;
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
  /** The app's own keys on this node, for the disclosure that shows them. */
  const appFields = ref<FieldRow[]>([]);
  const loading = ref(false);
  const saving = ref(false);
  const error = ref<string | null>(null);

  /** What was on disk when this was opened, to work out what changed. */
  let loadedKeys: string[] = [];

  /**
   * Everything a save would put in the file, as one comparable string.
   *
   * Over the raw text of each row rather than the value it parses to, which
   * errs towards saving: two spellings of one value read as a change and get
   * written. The other direction is the one that loses work, and this cannot
   * go that way — the write is a pure function of these inputs, so an equal
   * signature is an identical file.
   *
   * A field's *kind* is deliberately absent. It decides how a row is drawn and
   * never what is written; `changingAFieldKind.spec.ts` pins that down.
   */
  const written = () =>
    JSON.stringify([
      title.value,
      body.value,
      fields.value.map(r => [r.key.trim(), r.value]),
    ]);

  /**
   * The last state known to match what is on disk, or `null` when nothing is.
   *
   * `save()` used to write the file every time it was called, and it is called
   * on every blur — leaving a field, leaving the body. So opening a node and
   * clicking once rewrote it: a new `updated_at`, a new entry in the CRDT log,
   * a sync payload, for a file that had not changed. Harmless one at a time
   * and not harmless as the way the app behaves.
   */
  let clean: string | null = null;
  let token = 0;

  const nodeType = computed<string>(() => node.value?.node_type ?? '');

  const readOnlyRows = computed(() =>
    [...GOVERNED]
      .filter(k => k === 'type' || node.value?.properties?.[k] !== undefined)
      .map(k => ({
        key: k,
        value: k === 'type' ? nodeType.value : toText(node.value?.properties?.[k]),
      })),
  );

  /**
   * Open a node, and draw its empty fields the way the kind declares them.
   *
   * The shape arrives as a lookup rather than a list, because the caller does
   * not always know the kind: a backlink is an id and a title, and what it
   * points at is only known once it has been read. Asked here, after.
   */
  const open = async (id: string, shapeFor: (nodeType: string) => Shaped[] = () => []) => {
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

      /**
       * A value decides its own kind; a declaration only speaks for an empty one.
       *
       * `kindOf('')` answers text, and rightly — an empty string carries no
       * evidence. But `due_date` is an empty string on 126 tasks of 127 here,
       * so answering text there made declaring the field a date do nothing at
       * all on every task nobody had dated yet.
       *
       * The order is the point and never reverses: what the file holds wins
       * wherever the file holds anything.
       */
      const declared = new Map(shapeFor(found.node_type ?? '').map(f => [f.key, f.kind]));
      const row = (k: string): FieldRow => {
        const value = toText(props[k]);
        return {
          key: k,
          value,
          // Through `asFieldKind`, because a shape can come from a file
          // somebody edited and this decides which component draws the row.
          kind: value.trim() ? kindOf(props[k]) : asFieldKind(declared.get(k)),
          original: props[k],
        };
      };

      const type = found.node_type ?? '';
      fields.value = loadedKeys
        .filter(k => !isAppOwned(type, k) && !GOVERNED.has(k))
        .map(row);

      // Not shown by default and never lost. The disclosure exists because
      // "the app hid a field from me" is a worse failure than a noisy panel.
      appFields.value = loadedKeys.filter(k => isAppOwned(type, k)).map(row);

      // Just read off the disk, so by definition it matches.
      clean = written();
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
    draft.value = null;
    fields.value = [];
    appFields.value = [];
    loadedKeys = [];
    clean = null;
    error.value = null;
  };

  /**
   * A node being composed, which is not on disk yet.
   *
   * Creating used to write the file first and let the name be typed into it
   * afterwards, which is why the vault holds nodes with no title: every
   * abandoned create left one behind. A draft is the same screen with nothing
   * committed — leave without typing and there was never a file.
   *
   * It also settles the type before anything lands. Changing a type after the
   * fact would mean moving the file between folders, and here there is no file
   * to move.
   */
  const draft = ref<string | null>(null);

  /**
   * The shape of the kind, waiting to be filled in.
   *
   * Handed in rather than looked up, so this file keeps knowing nothing about
   * how a type's usual fields are worked out — and so the rule below can be
   * tested without a vault behind it.
   */
  const seed = (shape: Shaped[]): FieldRow[] =>
    shape.map(f => ({ key: f.key, value: '', kind: f.kind, original: undefined }));

  const startDraft = (nodeType: string, usual: Shaped[] = []) => {
    token++;
    node.value = { id: '', node_type: nodeType, title: '', content: '', properties: {} };
    title.value = '';
    body.value = '';
    fields.value = seed(usual);
    appFields.value = [];
    loadedKeys = [];
    clean = null;
    error.value = null;
    draft.value = nodeType;
  };

  /**
   * Free while it is still a draft, and only while it is.
   *
   * The rows come with the kind, so changing the kind changes them — anything
   * already typed into the old kind's fields is kept, because the person meant
   * it and a `colour` is a `colour` whichever kind is chosen.
   */
  const setDraftType = (nodeType: string, usual: Shaped[] = []) => {
    if (!draft.value || !node.value) return;
    draft.value = nodeType;
    node.value = { ...node.value, node_type: nodeType };

    const said = fields.value.filter(f => f.value.trim());
    const spoken = new Set(said.map(f => f.key));
    fields.value = [...said, ...seed(usual.filter(f => !spoken.has(f.key)))];
  };

  /**
   * Nothing said yet, so nothing to keep.
   *
   * A field with a name and no value says nothing: the rows are the kind's
   * shape offered up, not something the person did. Counting them would mean
   * opening the create screen for an `animal` and walking away wrote a file —
   * which is the whole failure drafts exist to end.
   */
  const draftIsEmpty = () =>
    !title.value.trim() &&
    !body.value.trim() &&
    !fields.value.some(f => f.value.trim());

  const addField = () =>
    fields.value.push({ key: '', value: '', kind: 'text', original: undefined });
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
  /**
   * Write a draft out for the first time, or do nothing if it is still blank.
   *
   * Returns the new path so the caller can select it; `null` means nothing was
   * written, which is the ordinary outcome of opening a draft and changing
   * your mind.
   */
  const commitDraft = async (): Promise<string | null> => {
    if (!draft.value || !node.value) return null;
    if (draftIsEmpty()) return null;

    const type = draft.value;
    const relPath = `${folderForType(type)}/${crypto.randomUUID()}.md`;
    saving.value = true;
    try {
      const properties: Record<string, unknown> = {};
      for (const row of fields.value) {
        const key = row.key.trim();
        if (!key || isAppOwned(type, key) || GOVERNED.has(key)) continue;
        // An offered row nobody filled in is not a field. Writing `species: ''`
        // into every new animal would make the offer into a commitment.
        if (!row.value.trim()) continue;
        properties[key] = valueOf(row.value, row.original);
      }

      await ns.writeNode({
        relPath,
        nodeType: type as never,
        title: title.value,
        properties,
        content: body.value,
        eventType: 'created',
      });

      draft.value = null;
      node.value = { ...node.value, id: relPath };
      loadedKeys = Object.keys(properties);
      clean = written();
      return relPath;
    } catch (e) {
      logger.error('[Things] Could not create node', e);
      error.value = String(e);
      return null;
    } finally {
      saving.value = false;
    }
  };

  const save = async () => {
    if (!node.value) return;
    // A draft has no file to patch; its first save is what creates one.
    if (draft.value) return;

    // Nothing to say. The panel asks for a save on every blur, and most blurs
    // follow no edit at all.
    const current = written();
    if (current === clean) return;

    saving.value = true;
    error.value = null;

    try {
      const patch: Record<string, unknown> = {};
      const kept = new Set<string>();

      const type = node.value.node_type ?? '';

      for (const row of fields.value) {
        const key = row.key.trim();
        if (!key || isAppOwned(type, key) || GOVERNED.has(key)) continue;
        kept.add(key);
        patch[key] = valueOf(row.value, row.original);
      }

      // Deletion by subtraction, and the guard on it. A hidden key is absent
      // from `fields`, so without `isAppOwned` here every field this panel
      // chose not to draw would be named `null` and erased from the file on
      // the first save — the panel would delete `pinned` by declining to show
      // it. Hiding and deleting have to stay different things.
      for (const key of loadedKeys) {
        if (isAppOwned(type, key) || GOVERNED.has(key)) continue;
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

      // The app's keys are still on disk; they simply were not in the patch.
      loadedKeys = [...kept, ...loadedKeys.filter(k => isAppOwned(type, k))];
      // Only now. A write that threw leaves the file behind the panel, and
      // marking it clean would make the next blur decline to try again.
      clean = current;
    } catch (e) {
      logger.error('[Things] Could not save node', e);
      error.value = String(e);
    } finally {
      saving.value = false;
    }
  };

  return {
    node, title, body, fields, appFields, readOnlyRows, nodeType, draft,
    loading, saving, error,
    open, close, addField, removeField, save, appOwnedKeys,
    startDraft, setDraftType, commitDraft, draftIsEmpty,
  };
}
