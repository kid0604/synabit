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

import synAvatar from '../../../../assets/syn-avatar.jpg';

defineProps<{
  editor: Editor | undefined;
  show: boolean;
  position: { top: number; left: number };
  /**
   * Offer "Ask Syn" as the first button, with this as its tooltip — or leave
   * it out, when this is `null`: Syn is off, or the app is locked. The editor
   * decides; see `synAsk` in `TiptapEditor.vue`.
   */
  askSynTitle?: string | null;
}>();

const emit = defineEmits<{
  (e: 'set-link'): void;
  /** Ask Syn about the selected text. */
  (e: 'ask-syn'): void;
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
        <!--
          Here rather than as a second floating button: this toolbar already
          appears over every selection in the editor, and two things appearing
          for one gesture would sit on top of each other. First, because it is
          the one button in the row that is not formatting.
        -->
        <template v-if="askSynTitle">
          <button
            @click="emit('ask-syn')"
            :title="askSynTitle"
            :aria-label="askSynTitle"
          >
            <img :src="synAvatar" alt="" class="w-5 h-5 rounded-full object-cover" />
          </button>
          <div class="bubble-divider" />
        </template>
        <button
          @click="editor.chain().focus().toggleBold().run()"
          :class="{ 'is-active': editor.isActive('bold') }"
          :title="$t('note.editor.bold')"
        >
          <BoldIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleItalic().run()"
          :class="{ 'is-active': editor.isActive('italic') }"
          :title="$t('note.editor.italic')"
        >
          <ItalicIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleUnderline().run()"
          :class="{ 'is-active': editor.isActive('underline') }"
          :title="$t('note.editor.underline')"
        >
          <UnderlineIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleStrike().run()"
          :class="{ 'is-active': editor.isActive('strike') }"
          :title="$t('note.editor.strikethrough')"
        >
          <StrikeThroughIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor.chain().focus().toggleHighlight().run()"
          :class="{ 'is-active': editor.isActive('highlight') }"
          :title="$t('note.editor.highlight')"
        >
          <Highlighter class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().toggleCode().run()"
          :class="{ 'is-active': editor.isActive('code') }"
          :title="$t('note.editor.inline_code')"
        >
          <Code class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="emit('set-link')"
          :class="{ 'is-active': editor.isActive('link') }"
          :title="$t('note.link_title')"
        >
          <LinkIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor.chain().focus().setTextAlign('left').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'left' }) }"
          :title="$t('note.editor.align_left')"
        >
          <AlignLeft class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('center').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'center' }) }"
          :title="$t('note.editor.align_center')"
        >
          <AlignCenter class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('right').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'right' }) }"
          :title="$t('note.editor.align_right')"
        >
          <AlignRight class="w-4 h-4" />
        </button>
        <button
          @click="editor.chain().focus().setTextAlign('justify').run()"
          :class="{ 'is-active': editor.isActive({ textAlign: 'justify' }) }"
          :title="$t('note.editor.align_justify')"
        >
          <AlignJustify class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <label
          :title="$t('note.editor.text_color')"
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
