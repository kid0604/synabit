<script setup lang="ts">
/**
 * A drop-down the formatting toolbar opens: the text-style choices, or the
 * list of things to insert.
 *
 * Follows the ARIA menu pattern because that is what a keyboard user expects
 * after pressing a button that says it has a popup: focus lands inside, the
 * arrow keys walk the list, Enter or Space picks, Escape goes back to the
 * button. It borrows the slash menu's look (`.slash-command-menu`) so the two
 * ways of reaching the same items look like the same thing.
 */
import { nextTick, onMounted, ref, type Component } from 'vue';

export interface ToolbarMenuItem {
  key: string;
  label: string;
  description?: string;
  icon?: Component;
  /** For a choice menu: whether this is the current one. */
  checked?: boolean;
}

const props = defineProps<{
  items: ToolbarMenuItem[];
  /** Accessible name of the menu, usually the button's. */
  label: string;
  /** Items are choices of which one is current, rather than actions. */
  radio?: boolean;
}>();

const emit = defineEmits<{
  (e: 'select', key: string): void;
  /** `refocus` is true when focus should go back to the opening button. */
  (e: 'close', refocus: boolean): void;
}>();

const itemEls = ref<HTMLButtonElement[]>([]);

const focusAt = (i: number) => {
  const n = itemEls.value.length;
  if (n === 0) return;
  itemEls.value[((i % n) + n) % n]?.focus();
};

const currentIndex = () => itemEls.value.findIndex((el) => el === document.activeElement);

onMounted(async () => {
  await nextTick();
  const checked = props.items.findIndex((it) => it.checked);
  focusAt(checked >= 0 ? checked : 0);
});

const onKeydown = (e: KeyboardEvent) => {
  const i = currentIndex();
  switch (e.key) {
    case 'ArrowDown': e.preventDefault(); focusAt(i + 1); break;
    case 'ArrowUp': e.preventDefault(); focusAt(i - 1); break;
    case 'Home': e.preventDefault(); focusAt(0); break;
    case 'End': e.preventDefault(); focusAt(itemEls.value.length - 1); break;
    case 'Escape': e.preventDefault(); e.stopPropagation(); emit('close', true); break;
    // Tab leaves the menu the way it leaves anything else; the menu just
    // stops being open behind it.
    case 'Tab': emit('close', false); break;
  }
};

/** Focus leaving the menu for somewhere that is not the menu closes it. */
const onFocusOut = (e: FocusEvent) => {
  const next = e.relatedTarget as Node | null;
  if (next && (e.currentTarget as HTMLElement).contains(next)) return;
  emit('close', false);
};
</script>

<template>
  <div
    class="slash-command-menu note-toolbar-menu"
    role="menu"
    :aria-label="label"
    @keydown="onKeydown"
    @focusout="onFocusOut"
  >
    <button
      v-for="item in items"
      :key="item.key"
      ref="itemEls"
      type="button"
      tabindex="-1"
      class="slash-menu-item"
      :class="{ 'is-selected': item.checked }"
      :role="radio ? 'menuitemradio' : 'menuitem'"
      :aria-checked="radio ? !!item.checked : undefined"
      @click="emit('select', item.key)"
    >
      <div v-if="item.icon" class="slash-menu-icon">
        <component :is="item.icon" class="w-4 h-4" />
      </div>
      <div class="slash-menu-text">
        <span class="slash-menu-title">{{ item.label }}</span>
        <span v-if="item.description" class="slash-menu-desc">{{ item.description }}</span>
      </div>
    </button>
  </div>
</template>
