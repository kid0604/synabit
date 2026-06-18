<script setup lang="ts">
import type { Editor } from '@tiptap/vue-3';
import {
  Bold as BoldIcon,
  Italic as ItalicIcon,
  Underline as UnderlineIcon,
  Strikethrough as StrikeThroughIcon,
  Highlighter,
  Code,
  Link as LinkIcon,
  AlignLeft,
  AlignCenter,
  AlignRight,
  AlignJustify,
  Palette
} from 'lucide-vue-next';

defineProps<{
  editor: Editor | undefined;
  show: boolean;
  position: { top: number; left: number };
}>();

const emit = defineEmits<{
  (e: 'set-link'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="bubble">
      <div
        v-if="show && editor"
        class="bubble-menu"
        :style="{ top: position.top + 'px', left: position.left + 'px' }"
        @mousedown.prevent
      >
        <button
          @click="editor.chain().focus().toggleBold().run()"
          :class="{ 'is-active': editor.isActive('bold') }"
          title="Bold"
        >
          <BoldIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleItalic().run()"
          :class="{ 'is-active': editor.isActive('italic') }"
          title="Italic"
        >
          <ItalicIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleUnderline().run()"
          :class="{ 'is-active': editor.isActive('underline') }"
          title="Underline"
        >
          <UnderlineIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleStrike().run()"
          :class="{ 'is-active': editor.isActive('strike') }"
          title="Strikethrough"
        >
          <StrikeThroughIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor.chain().focus().toggleHighlight().run()"
          :class="{ 'is-active': editor.isActive('highlight') }"
          title="Highlight"
        >
          <Highlighter class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleCode().run()"
          :class="{ 'is-active': editor.isActive('code') }"
          title="Inline Code"
        >
          <Code class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="emit('set-link')"
          :class="{ 'is-active': editor.isActive('link') }"
          title="Link"
        >
          <LinkIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor.chain().focus().setTextAlign('left').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'left' }) }"
          title="Align Left"
        >
          <AlignLeft class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('center').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'center' }) }"
          title="Align Center"
        >
          <AlignCenter class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('right').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'right' }) }"
          title="Align Right"
        >
          <AlignRight class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('justify').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'justify' }) }"
          title="Align Justify"
        >
          <AlignJustify class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <label
          title="Text Color"
          class="relative flex items-center justify-center p-1.5 rounded-sm hover:bg-slate-200 dark:hover:bg-slate-700 cursor-pointer text-slate-700 dark:text-slate-300 transition-colors tooltip-wrapper"
        >
          <Palette class="w-4 h-4" />
          <input 
            type="color" 
            @input="(e) => editor!.chain().focus().setColor((e.target as HTMLInputElement).value).run()" 
            :value="editor.getAttributes('textStyle').color || '#000000'"
            class="absolute opacity-0 inset-0 w-full h-full cursor-pointer"
          />
        </label>
      </div>
    </Transition>
  </Teleport>
</template>
