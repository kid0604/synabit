<script setup lang="ts">
/** A kept visual inside a note. See `SynVisualExtension`. */
import { computed } from 'vue';
import { NodeViewWrapper } from '@tiptap/vue-3';
import SynVisual from '../../../shared/components/SynVisual.vue';

const props = defineProps<{ node: any; editor: any }>();
const id = computed(() => String(props.node.attrs.visual || ''));
const vaultPath = computed(() => props.editor?.storage?.synVisual?.vaultPath || '');
</script>

<template>
  <NodeViewWrapper class="syn-visual-block" contenteditable="false">
    <SynVisual v-if="id && vaultPath" :visual-id="id" :vault-path="vaultPath" :keepable="false" />
  </NodeViewWrapper>
</template>
