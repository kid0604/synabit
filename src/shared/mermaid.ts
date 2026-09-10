/**
 * One Mermaid, one theme, one queue.
 *
 * # Why this file exists
 *
 * Two places in this app drew Mermaid diagrams and neither knew about the
 * other: `MessageBubble` for Syn's answers, `CodeBlockComponent` for a note's
 * code block. Both called `mermaid.initialize`, which is not a per-diagram
 * option — it is **global configuration for the library**. The chat set a
 * bespoke violet palette once, at module load. The note called
 * `applyMermaidTheme()` from `onMounted` of *every code block, in any
 * language*, and whichever ran last decided how every diagram in the app was
 * drawn from then on.
 *
 * So opening any note with any code block in it silently repainted the
 * conversation's diagrams. Nobody wrote that behaviour; it fell out of two
 * modules sharing a global and not knowing it.
 *
 * # And why the palette is gone
 *
 * The chat's twenty lines of `themeVariables` were being overwritten by the
 * first note anybody opened, so they were only ever in force in a session where
 * no note had been visited. Keeping them would mean two palettes for two
 * surfaces, which is the thing this file exists to stop. Mermaid's own `dark`
 * and `default`, following the app's theme, are what the app has actually been
 * showing most of the time.
 */
import { ref } from 'vue';
import mermaid from 'mermaid';

export type DiagramTheme = 'dark' | 'default';

const readTheme = (): DiagramTheme =>
  document.documentElement.classList.contains('dark') ? 'dark' : 'default';

/**
 * Which theme diagrams are drawn in, reactively.
 *
 * Watch it to redraw: a diagram is an SVG with its colours baked in, so a
 * theme switch is a re-render and not a stylesheet change.
 */
export const diagramTheme = ref<DiagramTheme>(readTheme());

/**
 * One observer for the whole app.
 *
 * `CodeBlockComponent` installed one per code block. A note with a dozen of
 * them installed a dozen observers, each firing on the same attribute, each
 * reconfiguring the same global library.
 */
if (typeof MutationObserver !== 'undefined') {
  new MutationObserver(() => {
    const now = readTheme();
    if (now !== diagramTheme.value) diagramTheme.value = now;
  }).observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
}

let configuredFor: DiagramTheme | null = null;

/**
 * Configure the library, and only when the answer has actually changed.
 *
 * `initialize` is cheap but it is not free, and more to the point it is the
 * call that used to be made from everywhere. Making it here, once per theme
 * change, is what stops one surface's idea of a diagram becoming everyone's.
 */
const configure = () => {
  if (configuredFor === diagramTheme.value) return;
  configuredFor = diagramTheme.value;
  mermaid.initialize({
    startOnLoad: false,
    theme: diagramTheme.value,
    // The app's font, so a diagram reads as part of the page it is in rather
    // than as something pasted from elsewhere.
    fontFamily: 'inherit',
  });
};

/**
 * Render one diagram, one at a time.
 *
 * # Why the queue
 *
 * Mermaid keeps state between the parse and the draw, and two renders in
 * flight at once come back wrong or not at all. The note editor already had
 * this queue and the chat did not — so a conversation with two diagrams in one
 * answer was racing, and a note open beside it raced with that.
 *
 * The `id` is Mermaid's own: it names a throwaway element after it, and leaves
 * that element in the body when a parse fails — which, in an editor, is most
 * keystrokes. Cleaning it up is part of rendering, not an afterthought.
 */
let queue: Promise<unknown> = Promise.resolve();

export type Drawn = { svg: string } | { error: string };

export const renderDiagram = (id: string, code: string): Promise<Drawn> => {
  const mine = queue.then(async (): Promise<Drawn> => {
    configure();
    try {
      const { svg } = await mermaid.render(id, code);
      return { svg };
    } catch (e) {
      // Mermaid names its scratch element after the id with a `d` in front.
      // Both are looked for because which one is left behind depends on how
      // far the parse got.
      document.getElementById(`d${id}`)?.remove();
      document.getElementById(id)?.remove();
      return { error: (e as { message?: string })?.message || 'Syntax error in the diagram' };
    }
  });

  // The chain must not break on a rejection, or every later diagram in the
  // session waits on a promise that never settles.
  queue = mine.catch(() => undefined);
  return mine;
};

/** An id nothing else will claim. */
let counter = 0;
export const diagramId = (prefix = 'diagram'): string =>
  `${prefix}-${Date.now().toString(36)}-${counter++}`;
