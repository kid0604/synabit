/**
 * Make the Android back button close what is on top instead of leaving.
 *
 * ## The problem
 *
 * Tauri's `WryActivity` handles back like this:
 *
 * ```kotlin
 * if (webView.canGoBack()) webView.goBack() else onBackPressed()  // finishes the app
 * ```
 *
 * The app is a single page with hash routing, and its modals are component
 * state rather than routes. So with a modal open, back does not close it — it
 * navigates the route *underneath* it, or, at the first entry, closes the app
 * outright. Losing a half-written note to a reflex is not something a user
 * forgives.
 *
 * ## The approach
 *
 * Work with that behaviour rather than around it: when a dismissible layer
 * opens, push a history entry for it. Back then consumes that entry, `popstate`
 * fires, and the layer closes with the route untouched. Closing by button
 * removes the entry again so history does not fill up with stale steps.
 *
 * This needs no Kotlin and behaves sensibly on desktop too, where the same
 * gesture exists as Alt+Left and the mouse back button.
 *
 * ## Deliberately not covered
 *
 * The last back press, when nothing is stacked, still closes the app with no
 * confirmation. Adding "press back again to exit" means keeping a sentinel
 * entry in history permanently, which interacts with routing in ways worth
 * testing on a device first. The common complaint is modals, and that is what
 * this fixes.
 */

import { onUnmounted, watch, type Ref } from 'vue';

interface Layer {
  id: number;
  dismiss: () => void;
}

/** Innermost layer last. */
const layers: Layer[] = [];
let nextLayerId = 1;

/**
 * How many upcoming `popstate` events are ones we caused ourselves.
 *
 * Removing our own history entry fires `popstate` exactly as a real back press
 * does, and acting on it would dismiss the layer *below* the one that just
 * closed.
 */
let selfInflictedPops = 0;
let listening = false;

function onPopState() {
  if (selfInflictedPops > 0) {
    selfInflictedPops--;
    return;
  }
  const top = layers.pop();
  if (top) top.dismiss();
}

function ensureListening() {
  if (listening) return;
  listening = true;
  window.addEventListener('popstate', onPopState);
}

/** What a caller gets back, for the one case the watcher cannot serve. */
export interface BackGuard {
  /**
   * Give up the claim on the back gesture and leave history untouched.
   *
   * For a click that closes the layer *and* navigates — picking an app out of
   * the sidebar's More menu does both — the ordinary close is wrong. The
   * router pushes its own entry a few microtasks after the watcher runs, so by
   * the time the browser carries out our `history.back()` the newest entry is
   * the navigation, and that is what gets undone. The app looks like it
   * ignored the click.
   *
   * Detaching first leaves our entry buried under the new route instead. It is
   * harmless there: it holds the same URL as the entry below it, so the press
   * that eventually consumes it goes where the user expects anyway.
   */
  detach: () => void;
}

/**
 * Register a layer that the back gesture should close.
 *
 * `isOpen` is watched, so the caller keeps owning the state and nothing has to
 * change about how the layer is opened or closed elsewhere. `dismiss` should do
 * exactly what the layer's own close button does.
 */
export function useBackGuard(isOpen: Ref<boolean>, dismiss: () => void): BackGuard {
  ensureListening();

  let layer: Layer | null = null;

  const release = () => {
    if (!layer) return;
    const index = layers.indexOf(layer);
    layer = null;
    if (index === -1) {
      // Already taken off the stack by `onPopState`, which means the back
      // gesture is what closed this. The history entry went with it.
      return;
    }
    layers.splice(index, 1);
    // Only the top of the stack owns the newest history entry. A layer closing
    // out of order leaves its entry behind, which costs one back press that
    // appears to do nothing — rare, and better than removing an entry that
    // belongs to a layer still on screen.
    if (index === layers.length) {
      selfInflictedPops++;
      window.history.back();
    }
  };

  watch(
    isOpen,
    (open) => {
      if (open && !layer) {
        if (layers.length === 0) {
          // Nothing was registered, so nothing legitimate can be waiting to be
          // suppressed. Any count still standing here is left over from a
          // `history.back()` whose `popstate` never arrived — which happens if
          // the entry we tried to remove was no longer the one on top. Left
          // alone it would swallow the next real back press, and the press
          // after that, for as long as the app runs. Clearing it at the one
          // moment it is provably meaningless bounds that to nothing.
          selfInflictedPops = 0;
        }
        layer = { id: nextLayerId++, dismiss };
        layers.push(layer);
        // No URL argument: the address stays exactly as it is, so the router
        // sees no navigation and only this module knows the entry is there.
        window.history.pushState({ synabitLayer: layer.id }, '');
      } else if (!open) {
        release();
      }
    },
    { immediate: true },
  );

  const detach = () => {
    if (!layer) return;
    const index = layers.indexOf(layer);
    if (index !== -1) layers.splice(index, 1);
    layer = null;
  };

  onUnmounted(() => {
    // Unmounting with the layer still open: take it off the stack so back does
    // not call into a dismiss that no longer belongs to anything, but leave
    // history alone — the component is going away for its own reasons and
    // navigating during teardown is how you get a loop.
    if (layer) {
      const index = layers.indexOf(layer);
      if (index !== -1) layers.splice(index, 1);
      layer = null;
    }
  });

  return { detach };
}

/** Is anything currently claiming the back gesture? Exposed for tests. */
export function backGuardDepth(): number {
  return layers.length;
}
