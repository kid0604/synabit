<script setup lang="ts">
/**
 * The actions a row offers, for a node of any kind.
 *
 * Modelled on `NoteContextMenu`, and deliberately not a copy of it. That menu
 * offers pinning, locking and opening in a window — all real, all specific to
 * notes, and none of them meaningful for a `book`. What is left when the
 * note-shaped entries are taken out is what any node can be asked to do.
 *
 * "Open in Tasks" is the interesting one. `routeForNodeType` answers `null` for
 * a type no app owns, and that `null` is the whole point: the entry appears
 * for a task and does not appear for an animal, without a list here saying
 * which is which.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ExternalLink, Edit2, Copy, Link2, Trash2, Pin, PinOff } from 'lucide-vue-next';
import { routeForNodeType } from '../nodeRoutes';
import { appName } from '../appRegistry';

const props = defineProps<{
  nodeId: string;
  nodeType: string;
  /**
   * Where the button that opened this sits, in screen coordinates.
   *
   * The menu is drawn over the page rather than inside the row, because a row
   * cannot contain it: the list scrolls, and the row's `content-visibility`
   * brings paint containment, which clips anything crossing its edge. Both
   * would cut the menu in half, which is exactly what they did.
   */
  at: { x: number; y: number };
  /** Whether this node is pinned, so the menu can offer the other one. */
  pinned?: boolean;
}>();

const emit = defineEmits<{
  open: [id: string];
  pin: [id: string];
  rename: [id: string];
  duplicate: [id: string];
  copyPath: [id: string];
  remove: [id: string];
}>();

const { t } = useI18n();

/** The app that owns this type, or nothing when none does. */
const owner = computed(() => {
  const route = routeForNodeType(props.nodeType);
  return route ? appName(route) : null;
});

/**
 * The menu's size, worked out rather than measured.
 *
 * Measuring means rendering once to find out how tall it is and then moving
 * it, which the eye catches. The menu is a fixed list of fixed-height rows, so
 * its height is known before it is drawn — and known is enough to decide which
 * way it opens.
 */
const WIDTH = 192;
const ITEM = 33;
const DIVIDER = 9;
const PADDING = 8;
const GAP = 4;

const height = computed(() => (owner.value ? 6 : 5) * ITEM + DIVIDER + PADDING);

/**
 * Kept on screen.
 *
 * A row near the bottom of a long list is where a menu is most likely to be
 * used and has least room to open downwards; a row at the right edge would
 * open one past the window. Both put the menu somewhere the mouse cannot go.
 */
const position = computed(() => {
  const below = props.at.y + GAP;
  const fits = below + height.value <= window.innerHeight - PADDING;
  return {
    left: `${Math.max(PADDING, Math.min(props.at.x - WIDTH, window.innerWidth - WIDTH - PADDING))}px`,
    top: fits
      ? `${below}px`
      : `${Math.max(PADDING, props.at.y - height.value - ITEM)}px`,
  };
});
</script>

<template>
  <Teleport to="body">
  <div
    :style="position"
    class="fixed w-48 z-[70] py-1 overflow-hidden rounded-lg
           bg-white dark:bg-[#2c2c2c] shadow-lg border border-gray-200 dark:border-gray-700"
  >
    <button
      v-if="owner"
      @click.stop="emit('open', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
    >
      <ExternalLink class="w-3.5 h-3.5 text-gray-400" />
      {{ t('things.open_in', { app: owner }) }}
    </button>

    <!--
      Pinning, which used to live only in the Notes menu because only notes
      could be pinned. Things pins whatever it shows, so it belongs to any
      node — and the key behind it became the app's on every kind.
    -->
    <button
      @click.stop="emit('pin', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
    >
      <component :is="pinned ? PinOff : Pin" class="w-3.5 h-3.5 text-gray-400" />
      {{ pinned ? t('things.unpin') : t('things.pin') }}
    </button>

    <button
      @click.stop="emit('rename', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
    >
      <Edit2 class="w-3.5 h-3.5 text-gray-400" />
      {{ t('things.rename') }}
    </button>

    <button
      @click.stop="emit('duplicate', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
    >
      <Copy class="w-3.5 h-3.5 text-gray-400" />
      {{ t('things.duplicate') }}
    </button>

    <button
      @click.stop="emit('copyPath', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
    >
      <Link2 class="w-3.5 h-3.5 text-gray-400" />
      {{ t('things.copy_path') }}
    </button>

    <div class="my-1 border-t border-gray-100 dark:border-gray-700"></div>

    <button
      @click.stop="emit('remove', nodeId)"
      class="w-full text-left px-3 py-2 text-xs whitespace-nowrap flex items-center gap-2
             text-red-600 dark:text-red-400
             hover:bg-red-50 dark:hover:bg-red-900/30 cursor-pointer"
    >
      <Trash2 class="w-3.5 h-3.5" />
      {{ t('things.delete') }}
    </button>
  </div>
  </Teleport>
</template>
