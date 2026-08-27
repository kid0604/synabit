<script setup lang="ts">
/**
 * What right-clicking a link offers.
 *
 * "Open" comes first because it is the reason most people right-click a link,
 * and "Remove" last and in red because it is the one that loses something.
 */
import { SquareArrowOutUpRight, Pencil, Unlink } from 'lucide-vue-next';

const props = defineProps<{
  show: boolean;
  top: number;
  left: number;
  href: string;
}>();

const emit = defineEmits<{
  (e: 'open'): void;
  (e: 'edit'): void;
  (e: 'remove'): void;
}>();

/** A link into the vault, as opposed to one out to the web. */
const isInternal = () => props.href.startsWith('synabit://');
</script>

<template>
  <Transition name="bubble">
    <div
      v-if="show"
      class="tc-ctx-menu"
      :style="{ position: 'absolute', top: top + 'px', left: left + 'px', zIndex: 100 }"
      @mousedown.prevent
    >
      <button @click="emit('open')" class="flex items-center gap-2">
        <SquareArrowOutUpRight class="w-3.5 h-3.5" />
        {{ isInternal() ? $t('note.link_open_note') : $t('note.link_open_external') }}
      </button>
      <button @click="emit('edit')" class="flex items-center gap-2">
        <Pencil class="w-3.5 h-3.5" />
        {{ $t('note.link_edit') }}
      </button>
      <button @click="emit('remove')" class="flex items-center gap-2 !text-red-500">
        <Unlink class="w-3.5 h-3.5" />
        {{ $t('note.link_remove') }}
      </button>
    </div>
  </Transition>
</template>
