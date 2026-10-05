<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, shallowRef } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeResizer } from '@vue-flow/node-resizer';
import { Editor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import { renderRichText } from '../richText';
import { followLink as followWebLink } from '../../../shared/syn/pane';
import { paint } from '../ink';
import RotateHandle from '../components/RotateHandle.vue';
import { useOnlySelected } from '../composables/useOnlySelected';

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    label: string;
    fontSize?: number;
    fontWeight?: string;
    fontStyle?: string;
    textAlign?: string;
    color?: string;
    backgroundColor?: string;
    opacity?: number;
    width?: number;
    rotation?: number;
    /** Just put down with the Text tool: open for typing straight away. */
    editing?: boolean;
  };
}>();

const emit = defineEmits<{
  (e: 'update:data', data: any): void;
  (e: 'rotate', degrees: number, final: boolean): void;
}>();

const { t } = useI18n();
const isEditing = ref(false);
const alone = useOnlySelected(() => props.selected);

/**
 * The words, written as Markdown and edited as rich text — the same editor
 * notes use, so bold, lists, headings and links work the way they do there,
 * shortcuts and all. One editor, made when editing starts and dropped when it
 * ends: a board of two hundred text boxes keeps two hundred plain blocks of
 * HTML, not two hundred editors.
 */
const editor = shallowRef<Editor | null>(null);
const html = computed(() => renderRichText(props.data.label ?? ''));

/** Open the editor with the caret at the end. */
function startEdit() {
  if ((props.data as any).locked || isEditing.value) return;
  isEditing.value = true;
  editor.value = new Editor({
    content: props.data.label ?? '',
    extensions: [StarterKit.configure({ link: { openOnClick: false } } as any), Markdown.configure({ breaks: true })],
    editorProps: {
      attributes: { class: 'wb-rich nodrag nopan', 'aria-label': t('whiteboard.a11y.kind.text') },
      handleKeyDown: (_view, event) => {
        if (event.key === 'Escape') {
          cancelEdit();
          return true;
        }
        return false;
      },
    },
    onBlur: () => finishEdit(),
  });
  // Once the editor is on the page: on a new box that is a frame later, after
  // the canvas has finished with the click that placed it.
  nextTick(() => requestAnimationFrame(() => {
    if (!editor.value) return;
    (editor.value.view.dom as HTMLElement).focus();
    editor.value.commands.focus('end');
  }));
}

function closeEditor() {
  editor.value?.destroy();
  editor.value = null;
}

function finishEdit() {
  // Escape, then the blur as the editor goes: one ending, one write.
  if (!isEditing.value || !editor.value) return;
  const markdown = ((editor.value.storage as any).markdown.getMarkdown() as string).trim();
  isEditing.value = false;
  closeEditor();
  if (markdown !== (props.data.label ?? '') || props.data.editing) emit('update:data', { label: markdown, editing: undefined });
}

function cancelEdit() {
  isEditing.value = false;
  closeEditor();
  if (props.data.editing) emit('update:data', { editing: undefined });
}

// Placed with the Text tool: typing goes into it at once, as it does into a
// new sticky note. It used to wait for a double-click, and the first letters
// typed went nowhere.
onMounted(() => { if (props.data.editing) startEdit(); });
onBeforeUnmount(closeEditor);

/**
 * The board's way of following a link: a vault link opens its app, an email
 * link the mail app, a web page where every link in the app opens. Outside a
 * board (a test), web pages only.
 */
const followLink = inject<(href: string) => void>('wbFollowLink', (href) => { void followWebLink(href); });

/** A link in the words opens where every link in the app opens, not in the board. */
function onClick(event: MouseEvent) {
  const link = (event.target as HTMLElement).closest('a');
  if (!link) return;
  event.preventDefault();
  event.stopPropagation();
  const href = link.getAttribute('href');
  if (href) followLink(href);
}

function onResizeEnd(event: any) {
  emit('update:data', { width: Math.round(event.params.width) });
}
</script>

<template>
  <div
    class="wb-text-node"
    :class="{ 'wb-text-node--editing': isEditing }"
    :style="{
      fontSize: (data.fontSize || 16) + 'px',
      fontWeight: data.fontWeight || 'normal',
      fontStyle: data.fontStyle || 'normal',
      textAlign: data.textAlign || 'left',
      color: paint(data.color) || 'inherit',
      backgroundColor: data.backgroundColor || 'transparent',
      // Kept as a percentage, like every other opacity on the board.
      opacity: (data.opacity ?? 100) / 100,
      width: (data.width || 200) + 'px',
      transform: data.rotation ? `rotate(${data.rotation}deg)` : undefined
    } as any"
    @dblclick.stop="startEdit"
  >
    <NodeResizer
      :is-visible="!!selected && !isEditing && !(data as any).locked"
      :min-width="80"
      :min-height="24"
      color="var(--wb-selection, var(--color-accent))"
      @resize-end="onResizeEnd"
    />

    <RotateHandle
      v-if="alone && !(data as any).locked && !isEditing"
      :node-id="id"
      :rotation="data.rotation"
      :label="$t('whiteboard.rotate')"
      @rotate="(deg: number, final: boolean) => emit('rotate', deg, final)"
    />

    <EditorContent v-if="isEditing && editor" :editor="editor" class="wb-text-editor" />
    <div v-else-if="html" class="wb-text-content wb-rich" @click="onClick" v-html="html" />
    <div v-else class="wb-text-content wb-text-placeholder">{{ $t('whiteboard.type_here2') }}</div>
  </div>
</template>

<style scoped>
.wb-text-placeholder {
  opacity: 0.5;
}
/* Rich text: tight, like writing on a board, not spaced like a document. */
.wb-rich :deep(p),
.wb-text-editor :deep(p) {
  margin: 0;
}
.wb-rich :deep(ul),
.wb-text-editor :deep(ul) {
  list-style: disc;
  padding-left: 1.2em;
  margin: 0.2em 0;
}
.wb-rich :deep(ol),
.wb-text-editor :deep(ol) {
  list-style: decimal;
  padding-left: 1.4em;
  margin: 0.2em 0;
}
.wb-rich :deep(h1),
.wb-text-editor :deep(h1) { font-size: 1.6em; font-weight: 700; margin: 0.1em 0; }
.wb-rich :deep(h2),
.wb-text-editor :deep(h2) { font-size: 1.35em; font-weight: 700; margin: 0.1em 0; }
.wb-rich :deep(h3),
.wb-text-editor :deep(h3) { font-size: 1.15em; font-weight: 600; margin: 0.1em 0; }
.wb-rich :deep(a),
.wb-text-editor :deep(a) {
  color: var(--color-accent);
  text-decoration: underline;
  cursor: pointer;
}
/* The light theme's accent is under 3:1 on the dark canvas: its own there. */
:global(.dark .wb-rich a),
:global(.dark .wb-text-editor a) {
  color: var(--color-accent-dark);
}
.wb-rich :deep(code),
.wb-text-editor :deep(code) {
  font-family: ui-monospace, monospace;
  font-size: 0.9em;
  padding: 0 0.25em;
  border-radius: 4px;
  background: color-mix(in srgb, currentColor 10%, transparent);
}
.wb-rich :deep(blockquote),
.wb-text-editor :deep(blockquote) {
  border-left: 3px solid color-mix(in srgb, currentColor 30%, transparent);
  padding-left: 0.6em;
  margin: 0.2em 0;
}
.wb-text-editor :deep(.ProseMirror) {
  outline: none;
  min-height: 1.4em;
  white-space: pre-wrap;
  word-break: break-word;
  cursor: text;
}
.wb-text-node {
  position: relative;
  width: 240px;
  min-width: 80px;
  height: 100%;
  padding: 8px 12px;
  border-radius: 8px;
  cursor: grab;
  border: 2px solid transparent;
  transition: border-color 0.15s, background-color 0.15s, opacity 0.15s, box-shadow 0.15s;
}
.wb-text-node--editing {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--color-accent) 15%, transparent);
}
.dark .wb-text-node--editing {
  border-color: var(--color-accent-dark);
  box-shadow: 0 0 0 3px color-mix(in oklab, var(--color-accent-dark) 20%, transparent);
}
.wb-text-content {
  white-space: pre-wrap;
  word-break: break-word;
  overflow-wrap: break-word;
}
.wb-text-input {
  width: 100%;
  height: 100%;
  min-height: 60px;
  background: transparent;
  border: none;
  padding: 0;
  outline: none;
  color: inherit;
  resize: none;
  font-family: inherit;
}
</style>
