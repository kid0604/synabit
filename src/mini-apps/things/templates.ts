import type { FieldKind } from '../../shared/fieldValue';
import type { SchemaField } from './composables/useThingsSchema';

/**
 * Ready-made kinds for somebody who has not designed one yet.
 *
 * Things can hold a list of anything, and an empty screen says none of that.
 * A teacher, a small shop, a reader — each of them knows what they want to
 * keep and would never guess that "design a kind" is how to start. A template
 * is that design done for them, and nothing more: creating one goes through
 * `designKind` in `ThingsApp.vue`, the path the kind designer itself uses, so
 * what it leaves behind is exactly what designing the same kind by hand
 * would — one `Schema/<kind>.md`, no nodes, nothing that cannot be renamed,
 * reshaped or removed the ordinary way afterwards.
 *
 * # Why names are translated when it is created, not when it is shown
 *
 * A kind's name and its field keys are what the files say — `type: book`,
 * `author: …` — and a vault is plain Markdown read by other editors, other
 * devices and Syn. Translating on screen would leave a Vietnamese user whose
 * files all say `author`. So the words are chosen once, in the language the
 * app is in when the template is picked, and from then on they are the user's
 * words like any other.
 *
 * # Why some fields are not here
 *
 * The title is every node's name, so "title" for a book and "item" for stock
 * are the title rather than a field that would duplicate it. And a status has
 * no list of allowed values: a schema declares how an empty box is drawn, not
 * what may go in it (see `useThingsSchema`), so the usual values are named in
 * the template's hint instead.
 */

export type TemplateId = 'book' | 'recipe' | 'student' | 'stock' | 'client' | 'reference';

export interface KindTemplate {
  id: TemplateId;
  /** A name in the kind icon table (`shared/views/nodeTypeIcon`). */
  icon: string;
  /**
   * The fields, by template-local id. The key written to disk is the
   * translation of `things.templates.<template>.fields.<id>`.
   */
  fields: ReadonlyArray<{ id: string; kind: FieldKind }>;
}

export const KIND_TEMPLATES: readonly KindTemplate[] = [
  {
    id: 'book',
    icon: 'book-open',
    fields: [
      { id: 'author', kind: 'text' },
      { id: 'status', kind: 'text' },
      { id: 'rating', kind: 'number' },
      { id: 'finished', kind: 'date' },
    ],
  },
  {
    id: 'recipe',
    icon: 'utensils',
    fields: [
      { id: 'ingredients', kind: 'list' },
      { id: 'minutes', kind: 'number' },
      { id: 'servings', kind: 'number' },
      { id: 'tags', kind: 'list' },
    ],
  },
  {
    id: 'student',
    icon: 'graduation-cap',
    fields: [
      { id: 'class', kind: 'text' },
      { id: 'parent_contact', kind: 'text' },
      { id: 'notes', kind: 'text' },
    ],
  },
  {
    id: 'stock',
    icon: 'package',
    fields: [
      { id: 'quantity', kind: 'number' },
      { id: 'price', kind: 'number' },
      { id: 'supplier', kind: 'text' },
    ],
  },
  {
    id: 'client',
    icon: 'briefcase',
    fields: [
      { id: 'company', kind: 'text' },
      { id: 'stage', kind: 'text' },
      { id: 'next_step', kind: 'date' },
    ],
  },
  {
    id: 'reference',
    icon: 'microscope',
    fields: [
      { id: 'source', kind: 'text' },
      { id: 'year', kind: 'number' },
      { id: 'link', kind: 'text' },
    ],
  },
];

/**
 * A translated word as a kind name or a field key.
 *
 * Lower case, like the kind designer makes a name (`KindDesigner.vue`), and
 * with spaces as underscores — which the designer does not do, and does not
 * need to, because a person typing a key chooses it. Here the words come from
 * a translation, and Vietnamese is almost all two-word phrases: `tác giả` as a
 * key would be two words to the query language (`query.rs` splits on
 * whitespace), so `sort:tác giả` could never sort by it.
 */
export function asKey(word: string): string {
  return word.trim().toLowerCase().replace(/\s+/g, '_');
}

type Translate = (key: string) => string;

/**
 * What a template becomes: the kind's name, and the fields to declare.
 *
 * `t` is the app's translator, so the words are the app language's at the
 * moment of creation. A field whose translation comes out empty, or the same
 * as one already taken, is left out rather than written as a blank or a
 * duplicate key.
 */
export function kindFromTemplate(template: KindTemplate, t: Translate): { nodeType: string; fields: SchemaField[] } {
  const nodeType = asKey(t(`things.templates.${template.id}.kind`));
  const seen = new Set<string>();
  const fields: SchemaField[] = [];
  for (const field of template.fields) {
    const key = asKey(t(`things.templates.${template.id}.fields.${field.id}`));
    if (!key || seen.has(key) || key === 'type' || key === 'title') continue;
    seen.add(key);
    fields.push({ key, kind: field.kind });
  }
  return { nodeType, fields };
}

/**
 * Whether picking a template may write a schema, or should only open the kind.
 *
 * A kind of that name already existing — declared, or merely carried by files
 * — is somebody's own, and a template has no business redeclaring its shape.
 * Opening it is the useful answer: they asked for books, and here are their
 * books.
 */
export function templateCreates(nodeType: string, existingKinds: readonly string[]): boolean {
  return !!nodeType && existingTemplateKind([nodeType], existingKinds) === null;
}

/** Case and Unicode composition aside, the same word. */
const sameWord = (w: string) => w.normalize('NFC').toLowerCase();

/**
 * The name a template's kind takes in each language the app speaks.
 *
 * A template picked in English writes `book`; picked again after switching to
 * Vietnamese it would write `sách` — a second kind for the same collection.
 * Asking about every translation is how it recognises its own work.
 */
export function templateKindNames(template: KindTemplate, translators: readonly Translate[]): string[] {
  const names = translators.map(t => asKey(t(`things.templates.${template.id}.kind`))).filter(Boolean);
  return [...new Set(names)];
}

/**
 * The kind already in the vault that one of these names means, as the vault
 * spells it — `Book` is the same kind as `book` to a person, if not to a file.
 */
export function existingTemplateKind(names: readonly string[], existingKinds: readonly string[]): string | null {
  const wanted = new Set(names.filter(Boolean).map(sameWord));
  return existingKinds.find(k => wanted.has(sameWord(k))) ?? null;
}
