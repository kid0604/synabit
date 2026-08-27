import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { nextTick } from 'vue';

/**
 * Does the Files app survive being set up?
 *
 * This test exists because a bug got past every other check in the repo. Type
 * checking passed, the bundle built, twenty-three hundred unit tests passed,
 * and the app then failed to open with
 *
 *     ReferenceError: Cannot access 'cellScale' before initialization
 *
 * A `watch(..., { immediate: true })` runs during setup, and it read a `ref`
 * declared further down the file. Nothing that inspects the code in pieces can
 * see that: the ordering only matters when the pieces run together, in order,
 * once.
 *
 * So this runs them. It asserts almost nothing about what appears — that is
 * what the other tests are for — and everything about the component reaching
 * the end of `setup` and its first render without throwing.
 *
 * # Why the mocks are shaped this way
 *
 * Everything crossing to Rust is stubbed at the boundary and answers with the
 * emptiest legal value. The point is to exercise this component's own wiring,
 * not the backend's; a mock that returned interesting data would test the mock.
 */

const invoked: string[] = [];

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string) => {
    invoked.push(command);
    switch (command) {
      case 'query_file_page':
        return { files: [], total: 0 };
      case 'file_text_backlog':
      case 'bulk_tag_files':
      case 'set_file_label':
      case 'watch_file_sources':
        return 0;
      default:
        // Every listing command in this app returns an array.
        return [];
    }
  }),
  convertFileSrc: (path: string) => `asset://${path}`,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(async () => null),
  ask: vi.fn(async () => false),
  message: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  readFile: vi.fn(async () => new Uint8Array()),
}));

// The PDF viewer pulls in pdfjs, which needs a worker jsdom has no use for.
// It is lazily imported by the viewer registry and never reached here.
vi.mock('pdfjs-dist/web/pdf_viewer.css', () => ({}));

const mounted: VueWrapper[] = [];

/**
 * Let the start-up chain finish.
 *
 * Opening the app is several awaits deep — read the sources, reindex them,
 * then ask for the first page — so a couple of ticks is not enough to see the
 * end of it.
 */
async function settle(rounds = 40) {
  for (let i = 0; i < rounds; i++) {
    await nextTick();
    await new Promise(resolve => setTimeout(resolve, 0));
  }
}

async function mountFilesApp() {
  const [{ default: FilesApp }, { i18n }] = await Promise.all([
    import('../FilesApp.vue'),
    import('../../../i18n'),
  ]);

  const wrapper = mount(FilesApp, {
    props: { vaultPath: '/vault' },
    global: {
      plugins: [i18n],
      // Not the subject: the app's own chrome, and a viewer that wants a real
      // canvas. Stubbing them keeps a failure here about this component.
      stubs: {
        NavButtons: true,
        FilesSidebar: true,
        FilesTabs: true,
        FilesInfoPanel: true,
      },
      provide: {
        pushNavigation: () => {},
      },
    },
  });
  mounted.push(wrapper);
  return wrapper;
}

describe('FilesApp', () => {
  beforeEach(() => {
    invoked.length = 0;
  });

  afterEach(() => {
    while (mounted.length) mounted.pop()?.unmount();
  });

  /// The whole point. A component whose setup throws never renders anything,
  /// and the failure surfaces as an unhandled rejection rather than as a
  /// broken screen — which is how it reached a user rather than a test.
  it('sets up and renders without throwing', async () => {
    const wrapper = await mountFilesApp();
    await nextTick();

    expect(wrapper.exists()).toBe(true);
    expect(wrapper.html().length).toBeGreaterThan(0);
  });

  /// Setup is only half of it: the store's watchers, the first page request and
  /// the thumbnail pass all land on the next few ticks.
  it('survives the work that lands after the first render', async () => {
    const wrapper = await mountFilesApp();
    await settle();

    expect(wrapper.exists()).toBe(true);
  });

  /// The list asks the database for a window rather than for everything. If
  /// this ever reverts to `query_files`, the fourteen-megabyte payload is back.
  it('asks for a page rather than the whole library', async () => {
    await mountFilesApp();
    await settle();

    expect(invoked, invoked.join(', ')).toContain('query_file_page');
    expect(invoked).not.toContain('query_files');
  });

  /// Changing the view must not throw either — the grid reads the cell scale
  /// that the original bug was about.
  it('switches between grid and list without throwing', async () => {
    const wrapper = await mountFilesApp();
    await nextTick();

    const app = wrapper.vm as unknown as { viewMode: string };
    app.viewMode = 'grid';
    await nextTick();
    app.viewMode = 'list';
    await nextTick();

    expect(wrapper.exists()).toBe(true);
  });
});
