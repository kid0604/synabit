/**
 * Turning what the user typed into what the vault's query engine understands.
 *
 * The Tasks app grew its own filter dialect in JavaScript — `is:transferred`,
 * `p:1`, `@Mai`, `prop:cost=100` — while the vault already had a real query
 * engine behind `run_node_query`, with dates, comparisons, sorting and limits,
 * used by the query blocks people write inside notes. Tasks never called it:
 * it stripped every token out and sent the bare words to the text index.
 *
 * So a note could ask "which tasks are overdue before September" and the Tasks
 * app could not.
 *
 * This translates one dialect into the other. Anything the JavaScript layer
 * still handles is taken *out* of the backend query rather than left in it,
 * because leaving it in would be worse than useless — `is:` means "node type"
 * to the engine, so `is:transferred` would ask for nodes of a type that does
 * not exist and match nothing at all.
 */

/** Tokens the JavaScript overlay owns, which must not reach the engine. */
const JS_OWNED = [
  // `is:` and `not:` on task-specific flags. The engine reads `is:` as a type.
  /\b(?:is|not):(?:transferred|tracked)\b/gi,
  // Priority shorthand. The engine wants the stored form, `priority:P1`.
  /\b(?:p|priority):[1-4]\b/gi,
  // Assignee. The engine has no `@` syntax and would search for the word.
  /(?:^|\s)@[^\s]+/g,
  // Custom property shorthand, including the bare existence form.
  /\bprop:[^:=\s]+(?:=[^\s]+)?/gi,
];

/**
 * The task-state shorthands, which the engine *does* have a word for.
 *
 * Translated rather than stripped: `status:` is the engine's own syntax, so
 * handing it over means the database does the narrowing instead of the browser.
 */
const STATUS_SHORTHANDS: [RegExp, string][] = [
  [/\bis:completed\b/gi, 'status:done'],
  [/\bis:todo\b/gi, 'status:todo'],
  [/\bis:in_progress\b/gi, 'status:in_progress'],
];

/**
 * Tokens that only the engine understands.
 *
 * Their presence is what decides whether a query is worth sending at all: a
 * search of purely JavaScript-owned tokens has nothing for the database to do,
 * and asking anyway would cost a round trip to be told everything matches.
 */
const ENGINE_ONLY = [
  /\bdate:[a-z-]+/i,
  // A comparison — `due_date:<2026-09-01`, `priority:>2`.
  /\b[a-z_]+:[<>]=?[^\s]+/i,
  /\bsort:-?[a-z_]+/i,
  /\blimit:\d+/i,
  /\bin:title\b/i,
];

export interface TranslatedQuery {
  /**
   * What to send to `run_node_query`, or `null` when there is nothing the
   * engine could add.
   */
  backend: string | null;
  /** What the browser still has to do, unchanged from what the user typed. */
  overlay: string;
}

/** Whether any free text survives once every token is removed. */
function hasFreeText(query: string): boolean {
  let rest = query;
  for (const pattern of JS_OWNED) rest = rest.replace(pattern, ' ');
  rest = rest
    .replace(/\b[a-z_]+:[^\s]+/gi, ' ')
    .replace(/(?:^|\s)[#-][^\s]+/g, ' ');
  return rest.trim().length > 0;
}

/**
 * Split what the user typed between the database and the browser.
 *
 * The overlay is returned verbatim: the JavaScript filters already read the
 * raw string and rewriting it for them would be a second translation to keep
 * in step with the first.
 */
export function translateTaskQuery(raw: string): TranslatedQuery {
  const query = (raw || '').trim();
  if (!query) return { backend: null, overlay: query };

  const engineHasWork = ENGINE_ONLY.some((p) => p.test(query))
    || hasFreeText(query)
    || STATUS_SHORTHANDS.some(([p]) => p.test(query))
    || /(?:^|\s)#[^\s]+/.test(query);

  if (!engineHasWork) return { backend: null, overlay: query };

  let backend = query;
  for (const [pattern, replacement] of STATUS_SHORTHANDS) {
    backend = backend.replace(pattern, replacement);
  }
  for (const pattern of JS_OWNED) {
    backend = backend.replace(pattern, ' ');
  }

  // `is:task` last and always: the engine needs to be told which nodes are
  // being asked about, and the user's own `is:` tokens have all been dealt
  // with by this point, so nothing can contradict it.
  backend = `is:task ${backend}`.replace(/\s+/g, ' ').trim();

  return { backend, overlay: query };
}
