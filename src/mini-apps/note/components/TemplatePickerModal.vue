<script setup lang="ts">
/**
 * Choose a template: to make a new note from it, or to drop it into the note
 * that is open (`/template`).
 *
 * The built-in ones first, then the user's own — every note in the vault's
 * `Templates/` folder (see `templates/noteTemplates.ts` for why a folder). The
 * preview on the right is the template as it will actually arrive, dates
 * filled in, so nobody has to guess what `{{date}}` turns into.
 *
 * The search box keeps focus while the arrow keys move through the list, the
 * way a command palette works: typing narrows, Enter takes the highlighted one.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { Search, LayoutTemplate, FileText } from 'lucide-vue-next';
import AppDialog from '../../../shared/components/AppDialog.vue';
import { i18n } from '../../../i18n';
import { logger } from '../../../utils/logger';
import {
  builtInTemplates,
  filterTemplates,
  instantiateTemplate,
  TEMPLATE_FOLDER,
  type NoteTemplate,
} from '../templates/noteTemplates';

const props = defineProps<{
  show: boolean;
  /** Make a new note, or insert into the open one. Only the wording differs. */
  mode: 'create' | 'insert';
  /** The user's own, from the note list; bodies not yet read. */
  userTemplates: NoteTemplate[];
  /** Read a user template's body. */
  loadBody: (id: string) => Promise<string>;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'choose', payload: { template: NoteTemplate; body: string }): void;
}>();

const { t } = useI18n();

const query = ref('');
const activeKey = ref<string | null>(null);
const bodies = ref<Record<string, string>>({});
const loadingKey = ref<string | null>(null);

const builtIns = computed(() => builtInTemplates(t));
const shownBuiltIns = computed(() => filterTemplates(builtIns.value, query.value));
const shownUser = computed(() => filterTemplates(props.userTemplates, query.value));
/** In the order the list shows them, for the arrow keys. */
const shown = computed(() => [...shownBuiltIns.value, ...shownUser.value]);

const active = computed(() => shown.value.find((tpl) => tpl.key === activeKey.value) ?? null);

watch(() => props.show, (open) => {
  if (!open) return;
  query.value = '';
  bodies.value = {};
  activeKey.value = shown.value[0]?.key ?? null;
}, { immediate: true });

// Keep something highlighted while the search narrows the list.
watch(shown, (list) => {
  if (!list.some((tpl) => tpl.key === activeKey.value)) activeKey.value = list[0]?.key ?? null;
});

const bodyOf = (tpl: NoteTemplate): string | null => tpl.body ?? bodies.value[tpl.key] ?? null;

// A user template's body is read the first time it is highlighted, not for
// the whole list up front: most people look at one or two.
watch(active, async (tpl) => {
  if (!tpl || bodyOf(tpl) !== null) return;
  loadingKey.value = tpl.key;
  try {
    const body = await props.loadBody(tpl.key);
    bodies.value = { ...bodies.value, [tpl.key]: body };
  } catch (e) {
    logger.error('Could not read a template note', e);
    bodies.value = { ...bodies.value, [tpl.key]: '' };
  } finally {
    if (loadingKey.value === tpl.key) loadingKey.value = null;
  }
});

const preview = computed(() => {
  const tpl = active.value;
  if (!tpl) return null;
  const body = bodyOf(tpl);
  if (body === null) return null;
  const filled = instantiateTemplate(tpl, body, new Date(), String(i18n.global.locale.value));
  return {
    title: filled.title,
    html: DOMPurify.sanitize(marked.parse(filled.body, { async: false }) as string),
  };
});

const choose = (tpl: NoteTemplate | null) => {
  if (!tpl) return;
  const body = bodyOf(tpl);
  // Still being read: the preview's spinner is showing, and a second press
  // once it has arrived does the job.
  if (body === null) return;
  emit('choose', { template: tpl, body });
};

const onSearchKeydown = (e: KeyboardEvent) => {
  const list = shown.value;
  if (list.length === 0) return;
  const i = list.findIndex((tpl) => tpl.key === activeKey.value);
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    activeKey.value = list[(i + 1) % list.length].key;
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    activeKey.value = list[(i - 1 + list.length) % list.length].key;
  } else if (e.key === 'Enter') {
    e.preventDefault();
    choose(active.value);
  }
};

const optionId = (key: string) => `note-template-${key.replace(/[^a-zA-Z0-9_-]/g, '_')}`;

const placeholders = { date: '{{date}}', time: '{{time}}', title: '{{title}}' };
</script>

<template>
  <AppDialog :show="show" labelledby="note-template-title" size="xl" panel-class="p-0 flex flex-col" @close="emit('close')">
    <div class="px-5 pt-5 pb-3 border-b border-border dark:border-border-dark">
      <h3 id="note-template-title" class="text-base font-semibold text-text dark:text-text-dark mb-3 flex items-center gap-2">
        <LayoutTemplate class="w-4 h-4 text-muted dark:text-muted-dark" />
        {{ mode === 'create' ? t('note.templates.new_from_template') : t('note.templates.insert_template') }}
      </h3>
      <div class="relative">
        <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted dark:text-muted-dark" />
        <input
          v-model="query"
          type="text"
          autofocus
          role="combobox"
          aria-autocomplete="list"
          aria-controls="note-template-list"
          :aria-expanded="true"
          :aria-activedescendant="activeKey ? optionId(activeKey) : undefined"
          :aria-label="t('note.templates.search')"
          :placeholder="t('note.templates.search')"
          class="w-full pl-8 pr-3 py-2 rounded-lg border border-border dark:border-border-dark bg-white dark:bg-[#2c2c2c] text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-accent/40"
          @keydown="onSearchKeydown"
        />
      </div>
    </div>

    <div class="flex flex-col md:flex-row min-h-0 md:h-[26rem]">
      <!-- The list -->
      <div
        id="note-template-list"
        role="listbox"
        :aria-label="t('note.templates.list_label')"
        class="md:w-64 shrink-0 overflow-y-auto max-h-60 md:max-h-none p-2 border-b md:border-b-0 md:border-r border-border dark:border-border-dark"
      >
        <div v-if="shownBuiltIns.length" role="group" :aria-label="t('note.templates.built_in')">
          <div class="px-2 pt-1 pb-1.5 text-xs font-semibold uppercase tracking-wider text-muted dark:text-muted-dark" aria-hidden="true">{{ t('note.templates.built_in') }}</div>
          <div
            v-for="tpl in shownBuiltIns"
            :id="optionId(tpl.key)"
            :key="tpl.key"
            role="option"
            :aria-selected="tpl.key === activeKey"
            class="px-2 py-1.5 rounded-lg cursor-pointer"
            :class="tpl.key === activeKey ? 'bg-accent/10' : 'hover:bg-gray-100 dark:hover:bg-surface-hover-dark'"
            @click="activeKey = tpl.key"
            @dblclick="choose(tpl)"
          >
            <div class="text-[13px] font-medium text-text dark:text-text-dark">{{ tpl.name }}</div>
            <div class="text-xs text-text-secondary dark:text-text-secondary-dark truncate">{{ tpl.description }}</div>
          </div>
        </div>

        <div v-if="shownUser.length || userTemplates.length === 0" role="group" :aria-label="t('note.templates.yours')" class="mt-2">
          <div class="px-2 pt-1 pb-1.5 text-xs font-semibold uppercase tracking-wider text-muted dark:text-muted-dark" aria-hidden="true">{{ t('note.templates.yours') }}</div>
          <div
            v-for="tpl in shownUser"
            :id="optionId(tpl.key)"
            :key="tpl.key"
            role="option"
            :aria-selected="tpl.key === activeKey"
            class="px-2 py-1.5 rounded-lg cursor-pointer flex items-center gap-2"
            :class="tpl.key === activeKey ? 'bg-accent/10' : 'hover:bg-gray-100 dark:hover:bg-surface-hover-dark'"
            @click="activeKey = tpl.key"
            @dblclick="choose(tpl)"
          >
            <FileText class="w-3.5 h-3.5 text-muted dark:text-muted-dark shrink-0" />
            <span class="text-[13px] font-medium text-text dark:text-text-dark truncate">{{ tpl.name }}</span>
          </div>
          <!-- The empty state is the help text: it is the only place anyone looks for it. -->
          <p v-if="userTemplates.length === 0" class="px-2 py-1 text-xs leading-relaxed text-text-secondary dark:text-text-secondary-dark">
            {{ t('note.templates.yours_empty', { folder: TEMPLATE_FOLDER, ...placeholders }) }}
          </p>
        </div>

        <p v-if="shown.length === 0" class="px-2 py-4 text-[13px] text-center text-text-secondary dark:text-text-secondary-dark">
          {{ t('note.templates.none_match') }}
        </p>
      </div>

      <!-- The preview -->
      <div class="flex-1 min-w-0 overflow-y-auto p-5 max-md:max-h-72" aria-live="polite">
        <template v-if="preview">
          <div class="text-xs text-muted dark:text-muted-dark mb-1">{{ t('note.templates.preview') }}</div>
          <h4 class="text-xl font-bold text-text dark:text-text-dark mb-3 break-words">{{ preview.title || t('note.untitled_note') }}</h4>
          <div class="prose prose-sm dark:prose-invert max-w-none" v-html="preview.html" />
        </template>
        <div v-else-if="active && loadingKey === active.key" class="text-[13px] text-text-secondary dark:text-text-secondary-dark">{{ t('note.editor.loading') }}</div>
      </div>
    </div>

    <div class="flex items-center justify-between gap-2 px-5 py-3 border-t border-border dark:border-border-dark">
      <p class="text-xs text-text-secondary dark:text-text-secondary-dark hidden sm:block">
        {{ t('note.templates.placeholders_hint', placeholders) }}
      </p>
      <div class="flex gap-2 ml-auto">
        <button type="button" class="btn-secondary" @click="emit('close')">{{ t('common.cancel') }}</button>
        <button type="button" class="btn-primary" :disabled="!preview" @click="choose(active)">
          {{ mode === 'create' ? t('note.templates.create') : t('note.templates.insert') }}
        </button>
      </div>
    </div>
  </AppDialog>
</template>
