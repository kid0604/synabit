import { describe, it, expect, vi, beforeEach } from 'vitest';

const { getNodeSummaries, writeNode, trashNode } = vi.hoisted(() => ({
  getNodeSummaries: vi.fn(),
  writeNode: vi.fn(),
  trashNode: vi.fn(),
}));

vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ getNodeSummaries, writeNode, trashNode }),
}));

import { useThingsSchema } from '../composables/useThingsSchema';
import {
  chosenIconName, iconForNodeType, iconNamed,
} from '../../../shared/views/nodeTypeIcon';

/**
 * A kind's declared shape, which is a file like everything else.
 *
 * Being a file is the point — it syncs, it diffs, it can be fixed in any
 * editor — and it is also why nothing in it can be trusted: it may have been
 * hand-edited, merged between two devices character by character, or written
 * by a version that spelled things differently.
 */
describe('the shape somebody set for a kind', () => {
  beforeEach(() => {
    getNodeSummaries.mockReset();
    writeNode.mockReset();
    trashNode.mockReset();
  });

  const load = async (rows: unknown[]) => {
    getNodeSummaries.mockResolvedValue(rows);
    const schema = useThingsSchema();
    await schema.load();
    return schema;
  };

  it('reads a shape and keeps its order', async () => {
    const schema = await load([{
      id: 'Schema/animal.md',
      title: 'animal',
      properties: { fields: [{ key: 'species', kind: 'text' }, { key: 'vaccinated_at', kind: 'date' }] },
    }]);

    expect(schema.schemaFor('animal')?.fields.map(f => f.key))
      .toEqual(['species', 'vaccinated_at']);
    expect(schema.kindFor('animal', 'vaccinated_at')).toBe('date');
  });

  /**
   * Until somebody has an opinion there is no file, and the observed shape is
   * the answer. That is what makes the offer right on the first day, before
   * anyone has opened an editor.
   */
  it('falls back to what the vault shows when nobody has said otherwise', async () => {
    const schema = await load([]);

    expect(schema.shapeFor('animal', ['species', 'colour'])).toEqual(['species', 'colour']);
    expect(schema.schemaFor('animal')).toBeNull();
  });

  it('prefers the declared shape once there is one', async () => {
    const schema = await load([{
      id: 'Schema/animal.md',
      title: 'animal',
      properties: { fields: [{ key: 'species', kind: 'text' }] },
    }]);

    expect(schema.shapeFor('animal', ['species', 'colour', 'màu'])).toEqual(['species']);
  });

  describe('a file that says something strange', () => {
    it('drops a kind it does not have', async () => {
      const schema = await load([{
        id: 'Schema/animal.md',
        title: 'animal',
        properties: { fields: [{ key: 'weight', kind: 'gigajoules' }] },
      }]);

      expect(schema.kindFor('animal', 'weight'), 'falls back rather than propagating').toBe('text');
    });

    it('drops an entry that is not a field at all', async () => {
      const schema = await load([{
        id: 'Schema/animal.md',
        title: 'animal',
        properties: { fields: [{ key: 'species' }, 7, null, { kind: 'date' }, { key: '  ' }] },
      }]);

      expect(schema.schemaFor('animal')?.fields.map(f => f.key)).toEqual(['species']);
    });

    /** A merge can leave one key twice; laying it out twice would be worse. */
    it('keeps the first of a duplicated key', async () => {
      const schema = await load([{
        id: 'Schema/animal.md',
        title: 'animal',
        properties: { fields: [{ key: 'species', kind: 'text' }, { key: 'species', kind: 'date' }] },
      }]);

      expect(schema.schemaFor('animal')?.fields).toHaveLength(1);
      expect(schema.kindFor('animal', 'species')).toBe('text');
    });

    it('survives fields being something other than a list', async () => {
      const schema = await load([{ id: 'Schema/x.md', title: 'animal', properties: { fields: 'species' } }]);

      expect(schema.schemaFor('animal')?.fields).toEqual([]);
    });

    /** A schema with no kind named cannot be matched to anything. */
    it('ignores a schema that does not say which kind it describes', async () => {
      const schema = await load([{ id: 'Schema/x.md', title: '   ', properties: { fields: [] } }]);

      expect(schema.schemas.value).toEqual([]);
    });
  });

  it('writes a new shape into its own file, named after the kind', async () => {
    const schema = await load([]);

    await schema.save('animal', [{ key: 'species', kind: 'text' }]);

    const sent = writeNode.mock.calls[0][0];
    // The folder comes from `folderForType` and the filename from the kind, so
    // `Schema/animal.md` says what it is to anyone reading the vault without
    // the app.
    expect(sent.relPath).toBe('Schema/animal.md');
    expect(sent.nodeType).toBe('schema');
    expect(sent.title, 'the kind it describes, readable in any editor').toBe('animal');
    expect(sent.eventType).toBe('created');
  });

  /** Editing an existing shape writes back to the same file, not a second one. */
  it('rewrites the file it already has', async () => {
    const schema = await load([{
      id: 'Schema/animal.md',
      title: 'animal',
      properties: { fields: [{ key: 'species', kind: 'text' }] },
    }]);

    await schema.save('animal', [{ key: 'colour', kind: 'text' }, { key: 'species', kind: 'text' }]);

    const sent = writeNode.mock.calls[0][0];
    expect(sent.relPath).toBe('Schema/animal.md');
    expect(sent.eventType).toBeUndefined();
    expect(sent.properties.fields.map((f: { key: string }) => f.key)).toEqual(['colour', 'species']);
  });

  /**
   * It asks for a node type, not a query.
   *
   * `get_node_summaries` matches `node_type` exactly, and this asked it for
   * `type:schema` — a kind by that literal name, which nothing is. Every
   * schema file was written correctly and never read back, so reordering a
   * field, adding one or removing one all looked like they had done nothing.
   * Three features, one wrong string, and nothing in type-check or lint can
   * see it because both are strings.
   */
  it('asks for the schema kind by name', async () => {
    await load([]);

    expect(getNodeSummaries).toHaveBeenCalledWith('schema');
  });

  /** And the write and the read have to name the same kind, or it never returns. */
  it('reads back what it just wrote', async () => {
    const schema = await load([]);
    await schema.save('animal', [{ key: 'species', kind: 'text' }]);

    expect(writeNode.mock.calls[0][0].nodeType).toBe('schema');
    const calls = getNodeSummaries.mock.calls;
    expect(calls[calls.length - 1][0]).toBe('schema');
  });

  /**
   * Throwing away what was declared, which is not deleting a kind.
   *
   * A kind exists because files say `type: x`. This removes the note *about*
   * those files and cannot be made to remove the files — which is why the
   * screen calls it forgetting a structure rather than deleting a kind.
   */
  describe('forgetting a structure', () => {
    it('trashes the declaration and nothing else', async () => {
      const schema = await load([{
        id: 'Schema/animal.md',
        title: 'animal',
        properties: { fields: [{ key: 'species', kind: 'text' }] },
      }]);

      await schema.remove('animal');

      expect(trashNode).toHaveBeenCalledWith({ relPath: 'Schema/animal.md' });
      expect(writeNode, 'no node of that kind is touched').not.toHaveBeenCalled();
    });

    /** The kind falls back to being described by its files. */
    it('leaves the kind described by what the vault holds', async () => {
      getNodeSummaries.mockResolvedValue([{
        id: 'Schema/animal.md',
        title: 'animal',
        properties: { fields: [{ key: 'species', kind: 'text' }] },
      }]);
      const schema = useThingsSchema();
      await schema.load();

      getNodeSummaries.mockResolvedValue([]);
      await schema.remove('animal');

      expect(schema.schemaFor('animal')).toBeNull();
      expect(schema.shapeFor('animal', ['colour', 'màu'])).toEqual(['colour', 'màu']);
    });

    it('does nothing for a kind that never had one', async () => {
      const schema = await load([]);

      await schema.remove('spaceship');

      expect(trashNode).not.toHaveBeenCalled();
    });
  });

  /**
   * The icon a kind was given, which lives beside its fields for the same
   * reason they do: it is an opinion about the kind, and the schema file is
   * where opinions about a kind go.
   */
  describe('the icon a kind was given', () => {
    // Index arithmetic rather than `.at(-1)`: this project's `lib` target
    // predates it, and a test is not a reason to move the whole build's floor.
    const lastWrite = () => writeNode.mock.calls[writeNode.mock.calls.length - 1][0];

    const withIcon = (icon: unknown) => [{
      id: 'Schema/animal.md',
      title: 'animal',
      properties: { fields: [{ key: 'species', kind: 'text' }], icon },
    }];

    it('is read back and published to the screens that draw it', async () => {
      getNodeSummaries.mockResolvedValue(withIcon('dog'));
      const schema = useThingsSchema();
      await schema.load();

      expect(schema.schemaFor('animal')?.icon).toBe('dog');
      expect(chosenIconName('animal'), 'the lists never heard about it').toBe('dog');
      expect(iconForNodeType('animal')).toBe(iconNamed('dog'));
    });

    /** Hand-edited, merged, or written by a version that had more icons. */
    it('reads a name this build does not know as no choice', async () => {
      getNodeSummaries.mockResolvedValue(withIcon('holographic-badger'));
      const schema = useThingsSchema();
      await schema.load();

      expect(schema.schemaFor('animal')?.icon).toBeNull();
      expect(chosenIconName('animal')).toBeNull();
    });

    it('is absent until somebody picks one', async () => {
      getNodeSummaries.mockResolvedValue([{
        id: 'Schema/animal.md', title: 'animal', properties: { fields: [] },
      }]);
      const schema = useThingsSchema();
      await schema.load();

      expect(schema.schemaFor('animal')?.icon).toBeNull();
    });

    /**
     * A patch of one key. Naming `fields` here would rewrite a list this call
     * has no opinion about, and their order is the thing somebody spent time
     * arranging.
     */
    it('is written without touching the fields', async () => {
      getNodeSummaries.mockResolvedValue(withIcon(null));
      const schema = useThingsSchema();
      await schema.load();

      await schema.saveIcon('animal', 'dog');

      const sent = lastWrite();
      expect(sent.relPath).toBe('Schema/animal.md');
      expect(sent.properties).toEqual({ icon: 'dog' });
      expect('fields' in sent.properties, 'the field order was rewritten').toBe(false);
    });

    /** `null` deletes the key, which is what "use the default again" means. */
    it('clears the choice rather than storing a blank one', async () => {
      getNodeSummaries.mockResolvedValue(withIcon('dog'));
      const schema = useThingsSchema();
      await schema.load();

      await schema.saveIcon('animal', null);
      expect(lastWrite().properties).toEqual({ icon: null });
    });

    /** A kind nobody has had an opinion about yet has no file to patch. */
    it('creates the schema file when this is the first opinion', async () => {
      getNodeSummaries.mockResolvedValue([]);
      const schema = useThingsSchema();
      await schema.load();

      await schema.saveIcon('book', 'reading');

      const sent = lastWrite();
      expect(sent.relPath).toBe('Schema/book.md');
      expect(sent.eventType).toBe('created');
    });
  });
});
