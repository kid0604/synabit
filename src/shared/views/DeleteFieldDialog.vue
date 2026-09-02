<script setup lang="ts">
/**
 * Taking a field off every node of a kind, values and all.
 *
 * The app's own `ConfirmModal` rather than a dialog of its own. The first
 * version was hand-built and it showed: three paragraphs of explanation, a
 * heading in the wrong grey, and a wall of copy in front of the case where
 * nothing happens at all. A confirmation is not the place to teach the screen.
 *
 * So the count decides everything. A field nothing carries is not a
 * destructive act and is not dressed as one; a field on 127 nodes says how
 * many, and says the values can still be found in each node's history — which
 * is true, and is not the same as an undo.
 */
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import ConfirmModal from '../components/ConfirmModal.vue';
import { logger } from '../../utils/logger';

const props = defineProps<{
  vaultPath: string;
  nodeType: string;
  field: string;
}>();

const emit = defineEmits<{ done: []; close: [] }>();

const { t } = useI18n();

/** Nodes that carry the key, or `null` while the answer is still coming. */
const count = ref<number | null>(null);
const busy = ref(false);

onMounted(async () => {
  try {
    const plan = await invoke<{ deleting: number }>('preview_delete_property', {
      nodeType: props.nodeType,
      key: props.field,
    });
    count.value = plan.deleting;
  } catch (e) {
    logger.error('[Things] Could not preview the deletion', e);
    emit('close');
  }
});

const message = computed(() => {
  if (count.value === null) return '';
  return count.value
    ? t('things.delete_field_count', { count: count.value })
    : t('things.delete_field_none', { field: props.field });
});

const apply = async () => {
  if (busy.value) return;
  busy.value = true;
  try {
    await invoke('delete_property', {
      vaultPath: props.vaultPath,
      nodeType: props.nodeType,
      key: props.field,
    });
    emit('done');
  } catch (e) {
    logger.error('[Things] Could not delete the field', e);
    emit('close');
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <!--
    Nothing is shown until the count is known: a confirmation whose numbers
    arrive after it does is one somebody has already started reading.
  -->
  <ConfirmModal
    :show="count !== null"
    :title="t('things.delete_field_title', { field, type: nodeType })"
    :message="message"
    :confirm-text="t('things.delete')"
    :cancel-text="t('things.cancel')"
    :is-destructive="!!count"
    @confirm="apply"
    @cancel="emit('close')"
  />
</template>
