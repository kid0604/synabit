import { ref } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';
import { folderForType } from '../../../shared/nodeRoutes';
import { logger } from '../../../utils/logger';
import { asFieldKind, type FieldKind } from '../../../shared/fieldValue';

/**
 * A kind's shape, once somebody has an opinion about it.
 *
 * Until then there is no file: `usualFieldsFor` reads the vault and answers
 * well enough, and a vault full of schema files nobody wrote is worse than no
 * schema at all. The file appears the first time an opinion is expressed —
 * a field reordered, a kind declared, an icon chosen — and from then on it is
 * what the app offers.
 *
 * What it never is, is a rule. Nothing here is consulted when a node is read,
 * validated or saved; the file on disk is the truth and can hold anything.
 * This only decides what a new node arrives holding and what the pickers put
 * in front of somebody first.
 */

export interface SchemaField {
  key: string;
  /**
   * How to draw the field before it has a value.
   *
   * Only used for an empty box. Once a value exists `kindOf` reads it from the
   * value itself and wins, because a declaration that disagrees with the file
   * is wrong about the file.
   */
  kind: FieldKind;
}

export interface TypeSchema {
  /** The node holding it, so it can be written back. */
  id: string;
  nodeType: string;
  fields: SchemaField[];
}

/** A schema file can be hand-edited or merged; nothing in it is trusted. */
function readFields(raw: unknown): SchemaField[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const fields: SchemaField[] = [];
  for (const entry of raw) {
    const key = typeof entry === 'string'
      ? entry
      : (entry as { key?: unknown })?.key;
    if (typeof key !== 'string' || !key.trim() || seen.has(key)) continue;
    seen.add(key);

    const declared = (entry as { kind?: unknown })?.kind;
    fields.push({
      key,
      kind: asFieldKind(declared),
    });
  }
  return fields;
}

export function useThingsSchema() {
  const ns = useNodeService();
  const schemas = ref<TypeSchema[]>([]);

  const load = async () => {
    try {
      // A node type, not a query. `get_node_summaries` matches `node_type`
      // exactly, so `type:schema` matched a kind by that literal name — which
      // is to say nothing. The files were written and never read back, and
      // every edit looked like it had done nothing at all.
      const rows = await ns.getNodeSummaries('schema');
      schemas.value = rows
        .map(row => {
          const props = (row.properties ?? {}) as Record<string, unknown>;
          return {
            id: row.id,
            // The kind it describes is the title, so a person reading
            // `Schema/animal.md` in any editor knows what they are looking at.
            nodeType: String(row.title ?? '').trim(),
            fields: readFields(props.fields),
          };
        })
        .filter(s => s.nodeType);
    } catch (e) {
      logger.error('[Things] Could not read schemas', e);
      schemas.value = [];
    }
  };

  const schemaFor = (nodeType: string): TypeSchema | null =>
    schemas.value.find(s => s.nodeType === nodeType) ?? null;

  /**
   * What a new node of this kind arrives holding.
   *
   * The declared shape when there is one, and the observed shape when there is
   * not — so the offer is right on the very first day, before anybody has
   * opened an editor.
   */
  const shapeFor = (nodeType: string, observed: string[]): string[] => {
    const declared = schemaFor(nodeType);
    return declared ? declared.fields.map(f => f.key) : observed;
  };

  /** The declared kind for a field, for drawing an empty box. */
  const kindFor = (nodeType: string, key: string): FieldKind | null =>
    schemaFor(nodeType)?.fields.find(f => f.key === key)?.kind ?? null;

  /**
   * Write the opinion down, creating the file if this is the first one.
   *
   * Fields are the whole list every time rather than a patch: their order is
   * the thing being edited, and an order cannot be expressed as a set of
   * independent changes.
   */
  const save = async (nodeType: string, fields: SchemaField[]) => {
    const existing = schemaFor(nodeType);
    const relPath = existing?.id
      ?? `${folderForType('schema')}/${nodeType}.md`;
    try {
      await ns.writeNode({
        relPath,
        nodeType: 'schema',
        title: nodeType,
        properties: { fields },
        content: '',
        ...(existing ? {} : { eventType: 'created' as const }),
      });
      await load();
    } catch (e) {
      logger.error('[Things] Could not save the schema', e);
    }
  };

  /**
   * Throw away what somebody declared about a kind.
   *
   * Not a way to delete a kind, and cannot be made into one. A kind exists
   * because files say `type: x`; this deletes the note *about* those files.
   * On `task` the kind stays and its structure goes back to whatever the 127
   * files turn out to hold. On `book`, which nothing carries, the declaration
   * was the only reason it appeared at all — so it stops appearing.
   *
   * Trashed rather than unlinked, like every other file this app removes.
   */
  const remove = async (nodeType: string) => {
    const existing = schemaFor(nodeType);
    if (!existing) return;
    try {
      await ns.trashNode({ relPath: existing.id });
      await load();
    } catch (e) {
      logger.error('[Things] Could not remove the schema', e);
    }
  };

  return { schemas, load, schemaFor, shapeFor, kindFor, save, remove };
}
