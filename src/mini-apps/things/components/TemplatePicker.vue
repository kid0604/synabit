<script setup lang="ts">
/**
 * "Start from a template": the ready-made kinds in `templates.ts`, to pick one.
 *
 * Only picks. What a template becomes, and whether it is written at all, is
 * decided in `ThingsApp.vue`, which owns the path the kind designer uses —
 * this dialog never writes anything, so cancelling it cannot leave anything
 * behind.
 *
 * Each card shows the fields in the words they will be written with, because
 * those are what the files will say: a Vietnamese user sees `tác_giả`, and
 * that is what they will get.
 */
import { useI18n } from 'vue-i18n';
import { Box } from 'lucide-vue-next';
import AppDialog from '../../../shared/components/AppDialog.vue';
import { iconNamed } from '../../../shared/views/nodeTypeIcon';
import { KIND_TEMPLATES, kindFromTemplate, type KindTemplate } from '../templates';

defineProps<{ show: boolean }>();

const emit = defineEmits<{
  pick: [template: KindTemplate];
  close: [];
}>();

const { t } = useI18n();

const titleId = `things-template-title-${Math.random().toString(36).slice(2, 9)}`;

const cards = KIND_TEMPLATES.map(template => ({
  template,
  icon: iconNamed(template.icon) ?? Box,
}));

/** The keys as they will be written, in the language the app is in now. */
const fieldsOf = (template: KindTemplate) =>
  kindFromTemplate(template, t).fields.map(f => f.key).join(', ');
</script>

<template>
  <AppDialog :show="show" :labelledby="titleId" size="lg" @close="emit('close')">
    <div class="px-5 py-4 border-b border-gray-100 dark:border-gray-700">
      <h3 :id="titleId" class="text-base font-semibold text-text dark:text-text-dark">
        {{ t('things.templates.title') }}
      </h3>
      <p class="mt-1 text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
        {{ t('things.templates.intro') }}
      </p>
    </div>

    <ul class="px-5 py-4 max-h-[60vh] overflow-y-auto grid grid-cols-1 sm:grid-cols-2 gap-2">
      <li v-for="card in cards" :key="card.template.id">
        <button
          type="button"
          @click="emit('pick', card.template)"
          class="w-full h-full text-left flex items-start gap-3 p-3 rounded-xl cursor-pointer
                 border border-gray-200 dark:border-gray-700/50
                 hover:border-accent/50 hover:bg-gray-50 dark:hover:bg-white/5 transition-colors"
        >
          <component :is="card.icon" class="w-5 h-5 mt-0.5 flex-shrink-0 text-accent" aria-hidden="true" />
          <span class="min-w-0">
            <span class="block text-sm font-medium text-text dark:text-text-dark">
              {{ t(`things.templates.${card.template.id}.title`) }}
            </span>
            <span class="block text-xs text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">
              {{ t(`things.templates.${card.template.id}.hint`) }}
            </span>
            <span class="block text-xs font-mono text-gray-500 dark:text-gray-400 mt-1 break-words">
              {{ fieldsOf(card.template) }}
            </span>
          </span>
        </button>
      </li>
    </ul>

    <div class="px-5 py-3 border-t border-gray-100 dark:border-gray-700 flex items-center justify-between gap-3">
      <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('things.templates.note') }}</p>
      <button type="button" class="btn-secondary" @click="emit('close')">
        {{ t('common.cancel') }}
      </button>
    </div>
  </AppDialog>
</template>
