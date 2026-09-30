/**
 * Note templates: a title pattern and a Markdown body, filled in when a note
 * is made from them.
 *
 * Two sources, one list:
 *
 * * **Built in.** A handful of shapes most people reach for — a meeting, a
 *   day, a lesson, a client visit. Their words live in the locale files
 *   (`note.templates.builtin.<id>`) so they arrive in the reader's language,
 *   and nothing about them is written to the vault until one is used.
 * * **The user's own.** Every note whose file sits in the vault's
 *   `Templates/` folder. That is the one convention, chosen over a `template`
 *   tag because a folder is something people already understand from every
 *   other notes app, it survives a sync to a device that has never seen this
 *   code, and "Save as template" can put a note there without touching its
 *   tags. The folder name is not translated: it is a path on disk, and a vault
 *   opened on two devices in two languages must agree on where it is.
 *
 * Templates are ordinary notes: search finds them, the note manager lists
 * them, and editing one is editing a note. Both lists mark them with a
 * "Template" badge, and the sidebar's Recent list leaves them out unless
 * somebody is searching (see `useNoteSearch`), so scaffolding does not crowd
 * out what was actually written lately.
 *
 * Placeholders are `{{date}}`, `{{time}}` and `{{title}}`. Anything else in
 * double braces is left exactly as written, so a note that happens to contain
 * `{{something}}` for its own reasons does not lose it on the way through.
 */

/** The vault folder whose notes are offered as templates. */
export const TEMPLATE_FOLDER = 'Templates';

/** The built-in templates, in the order the picker lists them. */
export const BUILT_IN_TEMPLATE_IDS = [
  'meeting',
  'daily',
  'project',
  'lesson',
  'reading',
  'client',
  'weekly',
] as const;

export type BuiltInTemplateId = (typeof BUILT_IN_TEMPLATE_IDS)[number];

export interface NoteTemplate {
  /** `builtin:<id>` or the template note's own path. */
  key: string;
  source: 'builtin' | 'user';
  /** What the picker calls it. */
  name: string;
  /** One line under the name; empty for the user's own. */
  description: string;
  /** Title of the note it makes, before placeholders are filled. */
  titlePattern: string;
  /**
   * The body before filling. `null` for a user template whose body has not
   * been read yet — the list is built from summaries, and a body is fetched
   * only for the one being previewed or used.
   */
  body: string | null;
  /** Tags the new note starts with (the template note's own, for the user's). */
  tags: string[];
}

/** Anything with a translate function shaped like vue-i18n's `t`. */
type Translate = (key: string, named?: Record<string, unknown>) => string;

/**
 * The built-in templates in the current language.
 *
 * The locale strings spell placeholders `{date}` and `{title}` — single
 * braces, because double ones are not valid vue-i18n syntax — and they are
 * handed back here as the literal double-brace form, so a built-in and a
 * user's template go through the same filling step.
 */
export function builtInTemplates(t: Translate): NoteTemplate[] {
  const literal = { date: '{{date}}', time: '{{time}}', title: '{{title}}' };
  return BUILT_IN_TEMPLATE_IDS.map((id) => {
    const base = `note.templates.builtin.${id}`;
    return {
      key: `builtin:${id}`,
      source: 'builtin' as const,
      name: t(`${base}.name`),
      description: t(`${base}.description`),
      titlePattern: t(`${base}.title`, literal),
      body: t(`${base}.body`, literal),
      tags: [],
    };
  });
}

/** Forward slashes, no leading `./` or `/` — ids arrive from Windows too. */
function normalisePath(path: string): string {
  return path.replace(/\\/g, '/').replace(/^\.?\//, '');
}

/**
 * Whether a note's path puts it in the templates folder, at any depth.
 *
 * Case-insensitive, because macOS and Windows are: a folder somebody made as
 * `templates` by hand is the same folder there, and should behave the same.
 */
export function isTemplatePath(path: string): boolean {
  const p = normalisePath(path).toLowerCase();
  return p.startsWith(`${TEMPLATE_FOLDER.toLowerCase()}/`);
}

/** The fields of a listed note that discovery needs. */
export interface TemplateCandidate {
  id: string;
  title: string;
  tags?: string[];
}

/** The user's own templates, from the note list, sorted by name. */
export function userTemplatesFrom(notes: TemplateCandidate[], untitled: string): NoteTemplate[] {
  return notes
    .filter((n) => isTemplatePath(n.id))
    .map((n) => ({
      key: n.id,
      source: 'user' as const,
      name: n.title?.trim() || untitled,
      description: '',
      titlePattern: n.title?.trim() || '',
      body: null,
      tags: [...(n.tags ?? [])],
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Templates whose name or description contains every word of the query.
 *
 * Diacritics are folded so `nhat ky` finds "Nhật ký" — most Vietnamese typed
 * into a search box on a laptop is typed without them.
 */
export function filterTemplates(list: NoteTemplate[], query: string): NoteTemplate[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return list;
  return list.filter((tpl) => {
    const hay = fold(`${tpl.name} ${tpl.description}`);
    return words.every((w) => hay.includes(w));
  });
}

function fold(s: string): string {
  return s
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/đ/g, 'd')
    .replace(/Đ/g, 'D')
    .toLowerCase();
}

export interface PlaceholderContext {
  /** When "now" is. Passed in so tests do not depend on the clock. */
  now: Date;
  /** The app's language, for the date and time. */
  locale: string;
  /** What `{{title}}` becomes. */
  title: string;
}

/**
 * The date as the reader would write it: "September 30, 2026" in English,
 * "30 tháng 9, 2026" in Vietnamese. Long rather than numeric because 03/04 is
 * two different days depending on who is reading.
 */
export function formatTemplateDate(now: Date, locale: string): string {
  try {
    return new Intl.DateTimeFormat(locale, { dateStyle: 'long' }).format(now);
  } catch {
    // An unknown locale tag throws; the date is still worth writing.
    return new Intl.DateTimeFormat('en', { dateStyle: 'long' }).format(now);
  }
}

function formatTemplateTime(now: Date, locale: string): string {
  try {
    return new Intl.DateTimeFormat(locale, { timeStyle: 'short' }).format(now);
  } catch {
    return new Intl.DateTimeFormat('en', { timeStyle: 'short' }).format(now);
  }
}

/**
 * Replace `{{date}}`, `{{time}}` and `{{title}}`, with or without spaces
 * inside the braces and in any case. Everything else is left alone.
 */
export function fillPlaceholders(text: string, ctx: PlaceholderContext): string {
  return text.replace(/\{\{\s*([a-zA-Z]+)\s*\}\}/g, (whole, name: string) => {
    switch (name.toLowerCase()) {
      case 'date': return formatTemplateDate(ctx.now, ctx.locale);
      case 'time': return formatTemplateTime(ctx.now, ctx.locale);
      case 'title': return ctx.title;
      default: return whole;
    }
  });
}

/**
 * The title and body a template makes, filled.
 *
 * The title is filled first, and `{{title}}` inside the title pattern means
 * the template's own name — there is no other title yet. The body's
 * `{{title}}` is then the finished title, so a heading that repeats it reads
 * the same as the note's name.
 */
export function instantiateTemplate(
  tpl: Pick<NoteTemplate, 'name' | 'titlePattern'>,
  body: string,
  now: Date,
  locale: string,
): { title: string; body: string } {
  const title = fillPlaceholders(tpl.titlePattern, { now, locale, title: tpl.name }).trim();
  return { title, body: fillPlaceholders(body, { now, locale, title }) };
}
