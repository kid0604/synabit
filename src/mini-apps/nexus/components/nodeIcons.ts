import { FileText, CheckSquare, Calendar, Tag, File, Users, Ghost } from 'lucide-vue-next';

/** One drawable piece of an icon: a subpath, and whether it is filled or stroked. */
export interface IconPart {
    path: Path2D;
    filled: boolean;
}

/**
 * The same glyphs the rest of Nexus uses for these types — see `getTypeIcon`
 * in NexusApp. A type absent from here simply has no icon, and stays a dot.
 */
const iconComponents: Record<string, unknown> = {
    note: FileText,
    task: CheckSquare,
    event: Calendar,
    tag: Tag,
    file: File,
    person: Users,
    ghost: Ghost,
};

const TAU = Math.PI * 2;

/**
 * Lucide ships icons as SVG components, and the canvas needs geometry.
 *
 * Every icon is built by `createLucideIcon`, which closes over its shape and
 * hands it down as an `iconNode` prop, so rendering the component once and
 * reading that prop yields exactly the artwork the rest of the UI draws —
 * rather than a second copy of it pasted in here to fall out of step.
 *
 * That is reaching into lucide's internals, so everything here fails soft: on
 * anything unexpected it returns null, and the caller draws the plain circle
 * it drew before icons existed.
 */
const buildParts = (component: unknown): IconPart[] | null => {
    try {
        const render = component as (p: object, c: object) => { props?: { iconNode?: unknown } };
        const iconNode = render({}, { slots: {}, attrs: {} })?.props?.iconNode;
        if (!Array.isArray(iconNode) || iconNode.length === 0) return null;

        const parts: IconPart[] = [];
        for (const entry of iconNode) {
            if (!Array.isArray(entry)) continue;
            const [tag, attrs] = entry as [string, Record<string, string>];
            if (!attrs) continue;

            // `fill: none` is lucide's default and means stroke it.
            const filled = !!attrs.fill && attrs.fill !== 'none';
            let path: Path2D | null = null;

            if (tag === 'path' && attrs.d) {
                path = new Path2D(attrs.d);
            } else if (tag === 'circle') {
                path = new Path2D();
                path.arc(+attrs.cx, +attrs.cy, +attrs.r, 0, TAU);
            } else if (tag === 'rect') {
                path = new Path2D();
                const [x, y, w, h] = [+attrs.x, +attrs.y, +attrs.width, +attrs.height];
                // roundRect is recent enough that an older WebView may not have
                // it; square corners on one icon is a fair trade for two lines.
                if (attrs.rx && typeof path.roundRect === 'function') {
                    path.roundRect(x, y, w, h, +attrs.rx);
                } else {
                    path.rect(x, y, w, h);
                }
            } else if (tag === 'line') {
                path = new Path2D();
                path.moveTo(+attrs.x1, +attrs.y1);
                path.lineTo(+attrs.x2, +attrs.y2);
            }

            // An unrecognised piece is skipped rather than failing the icon:
            // most of a glyph reads better than none of it.
            if (path) parts.push({ path, filled });
        }

        return parts.length ? parts : null;
    } catch {
        return null;
    }
};

const cache = new Map<string, IconPart[] | null>();

/**
 * Drawable parts of the icon for an item type, or null if it has none. Icons
 * are built once and kept: the draw loop asks for these on every frame.
 *
 * The geometry is in lucide's 24x24 box with a stroke width of 2, so a caller
 * scales by `size / 24` about the centre at (12, 12).
 */
export const iconPartsFor = (itemType: string): IconPart[] | null => {
    if (!cache.has(itemType)) {
        const component = iconComponents[itemType];
        cache.set(itemType, component ? buildParts(component) : null);
    }
    return cache.get(itemType) ?? null;
};
