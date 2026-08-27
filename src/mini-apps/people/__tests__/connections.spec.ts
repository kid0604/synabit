import { describe, it, expect } from 'vitest';
import { linkRemovalPatches } from '../composables/connections';

const person = (id: string, title: string, connections: any[] = [], relations: string[] = []) => ({
  id,
  title,
  properties: { connections, relations },
});

const link = (id: string, relation = 'friend') => ({ person_id: id, relation_type: relation });
const mention = (title: string, id: string) => `[${title}](synabit://person/${id})`;

describe('linkRemovalPatches', () => {
  it('names everyone who still points at the person going away', () => {
    // The regression this guards: deleting a person removed only their node,
    // leaving connections on everybody else aimed at a file that was gone.
    const people = [
      person('People/a.md', 'An', [link('People/gone.md')], [mention('Gone', 'People/gone.md')]),
      person('People/b.md', 'Binh', [link('People/a.md')]),
      person('People/c.md', 'Cuong', [link('People/gone.md'), link('People/a.md')]),
    ];

    const patches = linkRemovalPatches(people, 'People/gone.md');
    expect(patches.map(p => p.id)).toEqual(['People/a.md', 'People/c.md']);
  });

  it('removes the key entirely when no links are left', () => {
    // A write is a patch, so null is how it says "remove this". An empty
    // array would leave `connections: []` sitting in the file forever.
    const people = [person('People/a.md', 'An', [link('People/gone.md')], [mention('Gone', 'People/gone.md')])];
    const [patch] = linkRemovalPatches(people, 'People/gone.md');
    expect(patch.properties.connections).toBeNull();
    expect(patch.properties.relations).toBeNull();
  });

  it('keeps the links that had nothing to do with it', () => {
    const people = [
      person(
        'People/a.md',
        'An',
        [link('People/gone.md'), link('People/b.md', 'colleague')],
        [mention('Gone', 'People/gone.md'), mention('Binh', 'People/b.md')]
      ),
    ];
    const [patch] = linkRemovalPatches(people, 'People/gone.md');
    expect(patch.properties.connections).toEqual([
      { person_id: 'People/b.md', relation_type: 'colleague' },
    ]);
    expect(patch.properties.relations).toEqual([mention('Binh', 'People/b.md')]);
  });

  it('leaves alone anyone who never linked to them', () => {
    const people = [person('People/b.md', 'Binh', [link('People/a.md')])];
    expect(linkRemovalPatches(people, 'People/gone.md')).toEqual([]);
  });

  it('does not try to patch the person being removed', () => {
    const people = [
      person('People/gone.md', 'Gone', [link('People/a.md')]),
      person('People/a.md', 'An', [link('People/gone.md')]),
    ];
    const patches = linkRemovalPatches(people, 'People/gone.md');
    expect(patches.map(p => p.id)).toEqual(['People/a.md']);
  });

  it('survives a person with no links recorded at all', () => {
    const people = [{ id: 'People/a.md', title: 'An', properties: {} }, null];
    expect(linkRemovalPatches(people as any, 'People/gone.md')).toEqual([]);
  });
});
