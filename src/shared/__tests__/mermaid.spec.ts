import { describe, it, expect, vi, beforeEach } from 'vitest';

const initialize = vi.fn();
const draw = async (id: string, code: string) => {
  if (code.includes('boom')) throw new Error('Parse error on line 1');
  return { svg: `<svg data-id="${id}"><rect/></svg>` };
};
const render = vi.fn(draw);

vi.mock('mermaid', () => ({ default: { initialize, render } }));

/** A fresh module, and a mock that has forgotten whatever the last test made
 *  it do — `mockClear` forgets the calls, not the implementation. */
const load = async () => {
  vi.resetModules();
  initialize.mockClear();
  render.mockClear();
  render.mockImplementation(draw);
  return import('../mermaid');
};

beforeEach(() => { document.documentElement.className = ''; });

/**
 * Two surfaces drew Mermaid and neither knew about the other.
 *
 * `mermaid.initialize` is not a per-diagram option — it is global configuration
 * for the library. The chat set a bespoke palette once at module load; the note
 * editor called it from `onMounted` of *every code block, in any language*. So
 * opening any note with any code block in it silently repainted every diagram
 * in the conversation, and nobody wrote that behaviour: it fell out of two
 * modules sharing a global and not knowing it.
 */
describe('one Mermaid for the whole app', () => {
  it('configures the library once, not once per diagram', async () => {
    const { renderDiagram } = await load();

    await renderDiagram('a', 'flowchart LR\n A-->B');
    await renderDiagram('b', 'flowchart LR\n C-->D');
    await renderDiagram('c', 'flowchart LR\n E-->F');

    expect(initialize).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledTimes(3);
  });

  it('reconfigures when the theme actually changes, and not otherwise', async () => {
    const { renderDiagram, diagramTheme } = await load();

    await renderDiagram('a', 'x');
    expect(initialize.mock.calls[0][0]).toMatchObject({ theme: 'default' });

    diagramTheme.value = 'dark';
    await renderDiagram('b', 'x');
    expect(initialize).toHaveBeenCalledTimes(2);
    expect(initialize.mock.calls[1][0]).toMatchObject({ theme: 'dark' });

    // Same theme again: nothing to say.
    await renderDiagram('c', 'x');
    expect(initialize).toHaveBeenCalledTimes(2);
  });

  it('takes the theme from the document it starts in', async () => {
    document.documentElement.className = 'dark';
    const { diagramTheme } = await load();
    expect(diagramTheme.value).toBe('dark');
  });

  /**
   * Mermaid keeps state between the parse and the draw, and two renders in
   * flight come back wrong or not at all. The note editor had this queue; the
   * chat did not — so one answer with two diagrams was racing, and a note open
   * beside it raced with that.
   */
  it('draws one at a time', async () => {
    const { renderDiagram } = await load();

    let live = 0;
    let most = 0;
    render.mockImplementation(async (id: string) => {
      live++;
      most = Math.max(most, live);
      await new Promise(r => setTimeout(r, 1));
      live--;
      return { svg: `<svg data-id="${id}"/>` };
    });

    await Promise.all([
      renderDiagram('a', 'x'),
      renderDiagram('b', 'y'),
      renderDiagram('c', 'z'),
    ]);

    expect(most).toBe(1);
  });

  /**
   * A failed parse used to poison the chain: the queue was a promise, the
   * rejection was never caught, and every later diagram in the session waited
   * on something that would not settle.
   */
  it('keeps drawing after one fails to parse', async () => {
    const { renderDiagram } = await load();

    const bad = await renderDiagram('bad', 'boom');
    expect(bad).toHaveProperty('error');

    const good = await renderDiagram('good', 'flowchart LR\n A-->B');
    expect(good).toHaveProperty('svg');
  });

  /**
   * Mermaid renders into a throwaway element of its own and leaves it in the
   * body when the parse fails — which, in an editor, is most keystrokes.
   */
  it('clears up after itself when a parse fails', async () => {
    const { renderDiagram } = await load();

    const orphan = document.createElement('div');
    orphan.id = 'dleftover';
    document.body.appendChild(orphan);

    await renderDiagram('leftover', 'boom');
    expect(document.getElementById('dleftover')).toBeNull();
  });

  it('hands out an id nothing else will claim', async () => {
    const { diagramId } = await load();
    const ids = new Set(Array.from({ length: 50 }, () => diagramId()));
    expect(ids.size).toBe(50);
  });
});

/**
 * Both surfaces have to be on it, or the global is shared again by a different
 * route and the whole file is decoration.
 */
describe('who uses it', () => {
  it('is the only place either surface configures Mermaid', async () => {
    const bubble = (await import('../../mini-apps/messages/components/MessageBubble.vue?raw')).default;
    const block = (await import('../../mini-apps/note/CodeBlockComponent.vue?raw')).default;

    for (const [name, source] of [['MessageBubble', bubble], ['CodeBlockComponent', block]] as const) {
      // Comments are skipped: both files explain at length what they no longer
      // do, and a rule that fails because somebody wrote it down is not a rule.
      const code = source
        .split('\n')
        .filter(line => !/^\s*(\/\/|\*|<!--)/.test(line))
        .join('\n');

      expect(code, `${name} still imports mermaid directly`).not.toMatch(/^import mermaid from/m);
      expect(code, `${name} still configures the library`).not.toContain('mermaid.initialize');
      expect(code, `${name} does not use the shared renderer`).toContain('renderDiagram');
    }
  });

  /** The note column has the same problem a chat bubble did, and the answer
   *  was already sitting in `shared/components`. */
  it('lets a note open its diagram big, the way the conversation does', async () => {
    const block = (await import('../../mini-apps/note/CodeBlockComponent.vue?raw')).default;
    expect(block).toContain('DiagramViewer');
    expect(block).toContain('cursor: zoom-in');
  });
});

/**
 * A diagram carries its colours in a `<style>` element inside its own SVG.
 *
 * Which makes the app's Content Security Policy part of whether a diagram is
 * visible at all. Ours says `style-src 'self' 'unsafe-inline'` — but Tauri
 * rewrites that before serving the app: it stamps a nonce on every `<style>`
 * in `index.html` (there is one, painting the ground colour before any
 * stylesheet loads) and appends `'nonce-…'` to `style-src`. And **a nonce
 * anywhere in `style-src` makes `'unsafe-inline'` ignored**, so Mermaid's
 * style element — which has no nonce and cannot be given one, it arrives as
 * markup — is blocked. An unstyled `rect` is filled black by SVG's own
 * default, and every diagram in the packaged app came out as black slabs.
 *
 * Only in the packaged app: in development the front end is served by Vite
 * over http and no CSP is applied at all, so the machine running `tauri dev`
 * showed the diagrams correctly while the one running the dmg did not.
 *
 * `dangerousDisableAssetCspModification` keeps `style-src` as written here.
 * The nonce on `script-src` — the one that guards against injected scripts —
 * is left alone.
 */
describe('the policy a diagram has to live under', () => {
  it('leaves style-src as written, inline styles and all', async () => {
    const conf = (await import('../../../src-tauri/tauri.conf.json')).default as {
      app: { security: { csp: string; dangerousDisableAssetCspModification?: string[] } };
    };
    const security = conf.app.security;

    expect(security.csp).toContain("style-src 'self' 'unsafe-inline'");
    expect(
      security.dangerousDisableAssetCspModification,
      'Tauri will add a style-src nonce, and a nonce makes unsafe-inline ignored',
    ).toContain('style-src');
    expect(
      security.dangerousDisableAssetCspModification,
      'script-src keeps its nonce: that is the one that guards against injected scripts',
    ).not.toContain('script-src');
  });

  /** The `<style>` that earns the nonce, and why it cannot move to a file. */
  it('still paints the ground colour before any stylesheet loads', async () => {
    const html = (await import('../../../index.html?raw')).default;
    expect(html).toContain('<style>');
  });
});
