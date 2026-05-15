import { defineStore } from 'pinia';
import { ref, computed } from 'vue';

export interface NavEntry {
  app: string;         // 'note' | 'whiteboard' | 'task' | ...
  itemId?: string;     // ID of the item currently open (if any)
  scrollTop?: number;  // scroll position to restore
}

const MAX_STACK_SIZE = 10;

export const useNavigationStore = defineStore('navigation', () => {
  const backStack = ref<NavEntry[]>([]);
  const forwardStack = ref<NavEntry[]>([]);

  const canGoBack = computed(() => backStack.value.length > 0);
  const canGoForward = computed(() => forwardStack.value.length > 0);

  /**
   * Push current location onto the back stack before navigating away.
   * Clears the forward stack (just like a browser — navigating to a new page discards forward history).
   */
  function pushNavigation(entry: NavEntry) {
    backStack.value.push(entry);
    if (backStack.value.length > MAX_STACK_SIZE) {
      backStack.value.shift();
    }
    // New navigation invalidates forward history
    forwardStack.value = [];
  }

  /**
   * Go back: pop from back stack and push current location onto forward stack.
   * Returns the NavEntry to restore, or null if nothing to go back to.
   */
  function goBack(currentEntry: NavEntry): NavEntry | null {
    if (backStack.value.length === 0) return null;
    forwardStack.value.push(currentEntry);
    if (forwardStack.value.length > MAX_STACK_SIZE) {
      forwardStack.value.shift();
    }
    return backStack.value.pop()!;
  }

  /**
   * Go forward: pop from forward stack and push current location onto back stack.
   * Returns the NavEntry to restore, or null if nothing to go forward to.
   */
  function goForward(currentEntry: NavEntry): NavEntry | null {
    if (forwardStack.value.length === 0) return null;
    backStack.value.push(currentEntry);
    if (backStack.value.length > MAX_STACK_SIZE) {
      backStack.value.shift();
    }
    return forwardStack.value.pop()!;
  }

  return {
    backStack,
    forwardStack,
    canGoBack,
    canGoForward,
    pushNavigation,
    goBack,
    goForward,
  };
});
