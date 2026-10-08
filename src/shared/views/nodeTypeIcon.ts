import {
  FileText, CheckSquare, Calendar, Users, Zap, Palette, FolderOpen,
  Wallet, Rss, Filter, Box, Scale,
  Book, BookOpen, Bookmark, Lightbulb, Tag, Flag, Star, SquareCheck, Target,
  Clock, Bell, Trophy, Wrench, House, Building2, MapPin, Briefcase,
  GraduationCap, Sprout, Dog, Cat, Utensils, Coffee, Wine, Shirt, Heart, Pill,
  Dumbbell, Plane, Car, Bike, Music, Film, Camera, Image, Gift, ShoppingCart,
  Code, Terminal, Database, Server, Globe, Link, Microscope, TrendingUp, Package,
} from 'lucide-vue-next';
import { h, reactive, ref, render, watch, type Component } from 'vue';

/**
 * An icon for a node type, with a shape for the ones nobody has heard of.
 *
 * The fallback is the point. A vault may carry a type this app has never seen —
 * somebody else's tool wrote it, or the user invented one — and a view that
 * refused to draw a row without a known icon would be a list in the code
 * deciding which of the user's things are real.
 *
 * `Box` for the unknown ones: a container, unopinionated about what is in it,
 * and visibly different from the known types so the distinction reads without
 * needing an explanation.
 */
const ICONS: Readonly<Record<string, Component>> = {
  note: FileText,
  task: CheckSquare,
  project: CheckSquare,
  event: Calendar,
  person: Users,
  interaction: Users,
  quickcap: Zap,
  whiteboard: Palette,
  file: FolderOpen,
  finance_month: Wallet,
  feed_source: Rss,
  filter: Filter,
  decision: Scale,
};

/**
 * Every icon the library ships, by the name Lucide itself uses.
 *
 * Kebab-case rather than the JavaScript export name, because this string is
 * written into somebody's `Schema/<kind>.md` and read back by whatever version
 * is installed next. `file-text` is the name on lucide.dev, so a person
 * hand-editing that file can look it up; `FileText` is an implementation
 * detail of how this project happens to import it.
 *
 * Aliases are kept — the map holds around 1,900 names for around 1,700 icons,
 * and the extra names are Lucide's own trail of renames. A schema written
 * against an older name goes on resolving, which is the whole reason to store
 * their names rather than invent a second vocabulary.
 *
 * # Two halves
 *
 * The whole library is about 790 KB, and it used to be a namespace import
 * here — which put it in the load of every screen that draws a row. Now the
 * icons this app itself offers (the built-in kinds, the picker's first page,
 * the templates) are imported by name and resolve at once; the rest of the
 * library is fetched the first time somebody searches the picker or a schema
 * names an icon outside that set. Until it lands such a kind draws its
 * built-in icon, and redraws when it arrives: `iconChoiceVersion` moves.
 */
function kebab(exported: string): string {
  return exported
    .replace(/([a-z])([A-Z])/g, '$1-$2')
    .replace(/([A-Z])([A-Z][a-z])/g, '$1-$2')
    .replace(/([a-zA-Z])([0-9])/g, '$1-$2')
    .toLowerCase();
}

const byKebab = (exports: Record<string, unknown>): Map<string, Component> => {
  const map = new Map<string, Component>();
  for (const [exported, value] of Object.entries(exports)) {
    // The module also exports helpers, and every icon a second time with a
    // `Lucide` prefix and a third with an `Icon` suffix.
    if (!/^[A-Z]/.test(exported)) continue;
    if (exported.startsWith('Lucide') || exported.endsWith('Icon')) continue;
    if (typeof value !== 'function') continue;
    map.set(kebab(exported), value as Component);
  }
  return map;
};

/** The icons this app names itself — available without loading anything. */
const STATIC_BY_NAME: ReadonlyMap<string, Component> = byKebab({
  FileText, CheckSquare, Calendar, Users, Zap, Palette, FolderOpen,
  Wallet, Rss, Filter, Box, Scale,
  Book, BookOpen, Bookmark, Lightbulb, Tag, Flag, Star, SquareCheck, Target,
  Clock, Bell, Trophy, Wrench, House, Building2, MapPin, Briefcase,
  GraduationCap, Sprout, Dog, Cat, Utensils, Coffee, Wine, Shirt, Heart, Pill,
  Dumbbell, Plane, Car, Bike, Music, Film, Camera, Image, Gift, ShoppingCart,
  Code, Terminal, Database, Server, Globe, Link, Microscope, TrendingUp, Package,
});

/** The whole library, once it has been fetched. */
let fullByName: ReadonlyMap<string, Component> | null = null;
let catalogLoading: Promise<void> | null = null;

/**
 * Every name that can be stored, for a picker to search.
 *
 * Starts as the names available without loading anything and becomes the whole
 * library when `loadIconCatalog` resolves; reactive, so a picker that is open
 * when it lands widens on its own.
 */
export const ICON_NAMES: string[] = reactive([...STATIC_BY_NAME.keys()].sort());

/** Fetch the whole library. Safe to call any number of times. */
export function loadIconCatalog(): Promise<void> {
  if (fullByName) return Promise.resolve();
  catalogLoading ??= import('lucide-vue-next')
    .then((lucide) => {
      fullByName = byKebab(lucide as unknown as Record<string, unknown>);
      ICON_NAMES.splice(0, ICON_NAMES.length, ...[...fullByName.keys()].sort());
      reapplyChoices();
    })
    .catch((err) => {
      // Not cached, so the next ask tries again.
      catalogLoading = null;
      throw err;
    });
  return catalogLoading;
}

/**
 * Make sure every one of these names can be judged known or unknown
 * synchronously: fetches the library only if one of them is outside the set
 * available without it. A failed fetch leaves those names reading as unknown.
 */
export async function ensureIconsKnown(names: Iterable<unknown>): Promise<void> {
  if (fullByName) return;
  for (const name of names) {
    if (typeof name === 'string' && !STATIC_BY_NAME.has(name)) {
      await loadIconCatalog().catch(() => undefined);
      return;
    }
  }
}

export function iconNamed(name: string): Component | null {
  // Read so a template that asked before the library landed asks again after.
  void iconChoiceVersion.value;
  return STATIC_BY_NAME.get(name) ?? fullByName?.get(name) ?? null;
}

/**
 * What the picker offers before anybody types.
 *
 * Nineteen hundred icons is a search box, not a page to browse, and somebody
 * who does not yet know what they want needs a page. These are the ones people
 * keep things about, in the order they scan for them — so "the one for my
 * plants" is found by looking rather than by guessing the word Lucide used.
 */
export const SUGGESTED_ICONS: readonly string[] = [
  // Paper and making
  'file-text', 'book', 'book-open', 'bookmark', 'lightbulb', 'tag', 'flag', 'star',
  // Doing
  'square-check', 'target', 'clock', 'bell', 'calendar', 'trophy', 'wrench',
  // People and places
  'users', 'house', 'building-2', 'map-pin', 'briefcase', 'graduation-cap',
  // Living
  'sprout', 'dog', 'cat', 'utensils', 'coffee', 'wine', 'shirt', 'heart', 'pill', 'dumbbell',
  // Going
  'plane', 'car', 'bike',
  // Making and watching
  'music', 'film', 'camera', 'image', 'palette', 'gift', 'shopping-cart',
  // Work with machines
  'code', 'terminal', 'database', 'server', 'globe', 'link', 'rss', 'microscope',
  // Money and shape
  'wallet', 'trending-up', 'folder-open', 'filter', 'zap', 'box',
];

/**
 * What each kind was given, when somebody has said.
 *
 * Held here rather than threaded through every caller. `iconForNodeType` is
 * read by nine screens — every list, table, picker and graph — and all of them
 * want the answer synchronously while drawing a row. The choices come off disk
 * once and land here; a screen that renders before they arrive draws the
 * built-in icon and redraws when the map changes.
 */
const chosen = new Map<string, string>();

/** What was last asked for, including names still waiting on the library. */
let asked: Array<[string, string]> = [];

/** The registry's version, so a computed can depend on it. */
export const iconChoiceVersion = ref(0);

function reapplyChoices(): void {
  chosen.clear();
  for (const [nodeType, name] of asked) {
    // Ignoring a name this version does not know rather than drawing nothing:
    // a schema file outlives the build that wrote it, and can be hand-edited.
    if (STATIC_BY_NAME.has(name) || fullByName?.has(name)) chosen.set(nodeType, name);
  }
  iconChoiceVersion.value++;
}

export function setChosenIcons(icons: Iterable<[string, string]>): void {
  asked = [...icons];
  reapplyChoices();
  // A name outside the built-in set may still be one of Lucide's: fetch the
  // library, and the choice applies when it lands.
  if (!fullByName && asked.some(([, name]) => !STATIC_BY_NAME.has(name))) {
    loadIconCatalog().catch(() => undefined);
  }
}

/** What this kind was given, or `null` — for a picker to show as selected. */
export function chosenIconName(nodeType: string): string | null {
  return chosen.get(nodeType) ?? null;
}

export function iconForNodeType(nodeType: string): Component {
  // Read, not used. Nine screens call this straight from a template and none
  // of them touches the registry, so without this line a kind given a new icon
  // keeps its old one everywhere until something unrelated forces a redraw.
  // Touching the ref here makes every one of those renders depend on it.
  void iconChoiceVersion.value;

  const picked = chosen.get(nodeType);
  const named = picked ? iconNamed(picked) : null;
  if (named) return named;
  return ICONS[nodeType] ?? Box;
}

/** Whether this app ships a screen that understands the type. */
export function isKnownNodeType(nodeType: string): boolean {
  return nodeType in ICONS;
}

/**
 * The same icon as a bare SVG body, for a drawing surface Vue does not own.
 *
 * The graph is a d3 force simulation that appends its own elements, so it
 * cannot mount a component per node. Lucide keeps its path data inside a
 * closure — `FileText.iconNode` is not reachable — so the only way to the
 * shapes is to let Vue draw the icon once and read what it produced.
 *
 * Once per type, then cached: a graph of forty nodes is a handful of distinct
 * kinds, and the render is thrown away immediately.
 *
 * Returns the *contents* of the `<svg>`, not the element — paths on a 24×24
 * grid, which the caller places and scales itself.
 */
const markup = new Map<string, string>();

/**
 * Cleared whenever the choices change, or the graph would go on drawing the
 * icon a kind had before somebody changed it — cached under the kind's name,
 * which does not change when its icon does.
 */
watch(iconChoiceVersion, () => markup.clear());

export function iconBodyForNodeType(nodeType: string): string {
  const cached = markup.get(nodeType);
  if (cached !== undefined) return cached;

  let body = '';
  // Guarded because this runs for whatever type a vault happens to hold, and
  // an empty mark is a far better outcome than a graph that fails to draw.
  try {
    const holder = document.createElement('div');
    render(h(iconForNodeType(nodeType)), holder);
    body = holder.querySelector('svg')?.innerHTML ?? '';
    render(null, holder);
  } catch {
    body = '';
  }

  markup.set(nodeType, body);
  return body;
}
