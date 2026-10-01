<script setup lang="ts">
/**
 * Where `confirmDelete` asks its question. Mounted once, in App.vue.
 */
import { computed } from 'vue';
import ConfirmModal from './ConfirmModal.vue';
import { pendingDeleteQuestion, answerDeleteQuestion } from '../../composables/useConfirmDelete';
import { i18n } from '../../i18n';

const t = i18n.global.t;

const title = computed(() => {
  const q = pendingDeleteQuestion.value;
  if (!q) return '';
  if (q.count && q.count > 1) return t('common.delete_question_count', { count: q.count });
  return q.name ? t('common.delete_question', { name: q.name }) : t('common.delete_question_generic');
});

const body = computed(() =>
  pendingDeleteQuestion.value?.toTrash === false
    ? t('common.delete_question_body_no_trash')
    : t('common.delete_question_body'),
);
</script>

<template>
  <ConfirmModal
    :show="pendingDeleteQuestion !== null"
    :title="title"
    :message="body"
    :confirm-text="t('common.delete')"
    :cancel-text="t('common.cancel')"
    is-destructive
    @confirm="answerDeleteQuestion(true)"
    @cancel="answerDeleteQuestion(false)"
  />
</template>
