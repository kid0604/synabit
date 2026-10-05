import { cleanGlyph, type Glyph } from './glyph';
import { fold } from './boardSearch';

/**
 * The icons a board can be given: Lucide's, about 1,700 of them, the set the
 * app's own buttons are drawn from.
 *
 * Loaded the first time the picker asks, as a chunk of its own — nobody who
 * never opens it pays for it. Each icon component is asked for the drawing it
 * would make, and that drawing is what goes on the board (see `glyph.ts`), so
 * the board does not depend on this list afterwards.
 */
export interface IconEntry { name: string; words: string; glyph: Glyph }

let loading: Promise<IconEntry[]> | null = null;

/** Every icon, sorted by name. */
export function loadIcons(): Promise<IconEntry[]> {
  loading ??= import('lucide-vue-next').then((mod) => {
    const out: IconEntry[] = [];
    for (const make of Object.values(mod.icons ?? {})) {
      try {
        const node = (make as any)({}, { slots: {}, attrs: {} });
        const name = String(node?.props?.name ?? '');
        const glyph = cleanGlyph({ set: 'lucide', name, viewBox: [0, 0, 24, 24], parts: node?.props?.iconNode });
        if (name && glyph) out.push({ name, words: name.replace(/-/g, ' '), glyph });
      } catch {
        // An icon that cannot be read is one fewer to choose from.
      }
    }
    return out.sort((a, b) => a.name.localeCompare(b.name));
  }).catch((err) => {
    loading = null;
    throw err;
  });
  return loading;
}

/**
 * Vietnamese words for the icons people look for most, folded (no marks), to
 * the English the icons are named in. Not a dictionary: enough that "may chu"
 * or "máy chủ" finds a server.
 */
const VI: Record<string, string> = Object.assign(Object.create(null), {
  'may chu': 'server', 'nguoi dung': 'user', 'nguoi': 'user', 'nhom': 'users', 'co so du lieu': 'database', 'du lieu': 'database',
  'dam may': 'cloud', 'dien thoai': 'phone', 'may tinh': 'computer', 'may tinh xach tay': 'laptop', 'thu': 'mail', 'email': 'mail',
  'khoa': 'lock', 'chia khoa': 'key', 'nha': 'house', 'tim': 'search', 'tim kiem': 'search', 'cai dat': 'settings', 'lich': 'calendar',
  'tep': 'file', 'tai lieu': 'file text', 'thu muc': 'folder', 'anh': 'image', 'hinh anh': 'image', 'bieu do': 'chart', 'tien': 'dollar',
  'vi': 'wallet', 'gio hang': 'shopping cart', 'ngoi sao': 'star', 'sao': 'star', 'trai tim': 'heart', 'chuong': 'bell', 'mang': 'network',
  'wifi': 'wifi', 'bao mat': 'shield', 'khien': 'shield', 'xe': 'car', 'may bay': 'plane', 'dong ho': 'clock', 'ban do': 'map', 'ghim': 'pin',
  'am nhac': 'music', 'tai len': 'upload', 'tai xuong': 'download', 'thung rac': 'trash', 'but': 'pen', 'sach': 'book', 'mui ten': 'arrow',
  'kiem tra': 'check', 'canh bao': 'triangle alert', 'thong tin': 'info', 'cong ty': 'building', 'toa nha': 'building', 'dien': 'zap',
  'tin nhan': 'message', 'binh luan': 'message', 'chia se': 'share', 'lien ket': 'link', 'bong den': 'lightbulb', 'y tuong': 'lightbulb',
  'muc tieu': 'target', 'co': 'flag', 'qua': 'gift', 'ma': 'code', 'cpu': 'cpu', 'o cung': 'hard drive', 'may in': 'printer',
});

/**
 * Icons whose names hold every word asked for, the ones that begin with it
 * first: "data" finds `database` before `hard-drive-download`. Icons are
 * named in English; common Vietnamese words are looked up in `VI` first.
 */
export function findIcons(icons: IconEntry[], query: string): IconEntry[] {
  const folded = fold(query).trim().replace(/\s+/g, ' ');
  // Read as Vietnamese only when it was written with marks, or is a phrase:
  // "co" or "ma" alone could as well be the start of an English name.
  const marked = fold(query) !== query.toLowerCase();
  const phrase = marked || folded.includes(' ') ? VI[folded] : undefined;
  const asked = phrase ?? (marked ? folded.split(' ').map((w) => VI[w] ?? w).join(' ') : folded);
  const words = asked.split(/[\s-]+/).filter(Boolean);
  if (!words.length) return icons;
  const hits = icons.filter((i) => words.every((w) => i.words.includes(w)));
  const starts = (i: IconEntry) => (i.name.startsWith(words[0]) ? 0 : i.words.split(' ').some((p) => p.startsWith(words[0])) ? 1 : 2);
  return hits.sort((a, b) => starts(a) - starts(b) || a.name.length - b.name.length);
}
