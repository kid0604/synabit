<script setup lang="ts">
/**
 * Standing instructions about arriving articles.
 *
 * A feed that is ninety per cent useful and ten per cent press releases is
 * otherwise a choice between reading the press releases and unsubscribing.
 * Rules add rather than override each other — there is no order to learn,
 * because a rule list is not a program.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { X, Filter, Plus, Trash2, Loader2 } from 'lucide-vue-next';
import { useFocusTrap } from '../composables/useFocusTrap';
import { useArticleService } from '../composables/useArticleService';
import type { FeedRule, FeedSource } from '../types/feed.types';

const props = defineProps<{ sources: FeedSource[] }>();
const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const feedService = useArticleService();

const dialog = ref<HTMLElement | null>(null);
useFocusTrap(dialog);

const rules = ref<FeedRule[]>([]);
const loading = ref(true);
const applying = ref(false);
const appliedCount = ref<number | null>(null);

const FIELDS: { id: FeedRule['field']; labelKey: string }[] = [
  { id: 'any', labelKey: 'feeds.rule_field_any' },
  { id: 'title', labelKey: 'feeds.rule_field_title' },
  { id: 'summary', labelKey: 'feeds.rule_field_summary' },
  { id: 'author', labelKey: 'feeds.rule_field_author' },
];

const load = async () => {
  loading.value = true;
  try {
    rules.value = await feedService.getRules();
  } finally {
    loading.value = false;
  }
};
watch(() => props.sources, load, { immediate: true });

const addRule = () => {
  rules.value = [
    ...rules.value,
    {
      id: `rule-${Date.now()}-${Math.floor(Math.random() * 1000)}`,
      name: '',
      enabled: true,
      sourceIds: [],
      field: 'any',
      contains: '',
      markRead: false,
      star: false,
      mute: false,
      tag: '',
    },
  ];
};

const removeRule = (id: string) => {
  rules.value = rules.value.filter(r => r.id !== id);
};

/** A rule with nothing to match on would apply to everything, so it is dropped. */
const usableRules = () => rules.value.filter(r => r.contains.trim().length > 0);

const save = async () => {
  await feedService.saveRules(usableRules());
  emit('saved');
};

const applyNow = async () => {
  applying.value = true;
  appliedCount.value = null;
  try {
    await feedService.saveRules(usableRules());
    appliedCount.value = await feedService.applyRules();
  } finally {
    applying.value = false;
  }
};

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') emit('close');
};
</script>

<template>
  <div ref="dialog" class="fixed inset-0 z-[200] flex items-center justify-center" role="dialog" aria-modal="true" :aria-label="t('feeds.rules')" tabindex="-1" @keydown="handleKeydown">
    <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" @click="emit('close')"></div>

    <div class="relative w-full max-w-2xl mx-4 bg-white dark:bg-[#1a1a1a] rounded-2xl shadow-2xl border border-gray-200 dark:border-[#2c2c2c] overflow-hidden animate-in">
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-[#2c2c2c]">
        <h2 class="text-lg font-bold flex items-center gap-2">
          <Filter class="w-5 h-5 text-orange-500" />
          {{ t('feeds.rules') }}
        </h2>
        <button @click="emit('close')" class="p-1.5 rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="t('feeds.a11y_close')">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="px-6 py-5 space-y-4 max-h-[60vh] overflow-y-auto">
        <p class="text-xs text-gray-400 dark:text-gray-500">{{ t('feeds.rules_hint') }}</p>

        <div v-if="loading" class="flex justify-center py-8">
          <Loader2 class="w-5 h-5 animate-spin text-orange-500" />
        </div>

        <p v-else-if="rules.length === 0" class="py-6 text-center text-sm text-gray-400">{{ t('feeds.rules_empty') }}</p>

        <div
          v-for="rule in rules"
          :key="rule.id"
          class="p-4 rounded-xl border border-border dark:border-border-dark space-y-3"
          :class="{ 'opacity-60': !rule.enabled }"
        >
          <div class="flex items-center gap-2">
            <input v-model="rule.enabled" type="checkbox" class="w-4 h-4 shrink-0 accent-orange-500" :aria-label="rule.name || t('feeds.rule_name')" />
            <input
              v-model="rule.name"
              :placeholder="t('feeds.rule_name_placeholder')"
              class="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
            />
            <button @click="removeRule(rule.id)" class="p-1.5 rounded-lg text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors" :aria-label="t('feeds.rule_delete')">
              <Trash2 class="w-4 h-4" />
            </button>
          </div>

          <div class="flex flex-wrap items-center gap-2 text-sm text-gray-600 dark:text-gray-400">
            <span>{{ t('feeds.rule_when') }}</span>
            <select v-model="rule.field" class="px-2 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark">
              <option v-for="field in FIELDS" :key="field.id" :value="field.id">{{ t(field.labelKey) }}</option>
            </select>
            <span>{{ t('feeds.rule_contains') }}</span>
            <input
              v-model="rule.contains"
              :placeholder="t('feeds.rule_contains_placeholder')"
              class="flex-1 min-w-[140px] px-2.5 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
            />
          </div>

          <div class="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-gray-600 dark:text-gray-400">
            <span>{{ t('feeds.rule_then') }}</span>
            <label class="flex items-center gap-1.5 cursor-pointer">
              <input v-model="rule.markRead" type="checkbox" class="w-4 h-4 accent-orange-500" />
              {{ t('feeds.rule_mark_read') }}
            </label>
            <label class="flex items-center gap-1.5 cursor-pointer">
              <input v-model="rule.star" type="checkbox" class="w-4 h-4 accent-orange-500" />
              {{ t('feeds.rule_star') }}
            </label>
            <label class="flex items-center gap-1.5 cursor-pointer">
              <input v-model="rule.mute" type="checkbox" class="w-4 h-4 accent-orange-500" />
              {{ t('feeds.rule_mute') }}
            </label>
            <label class="flex items-center gap-1.5">
              {{ t('feeds.rule_tag') }}
              <input
                v-model="rule.tag"
                :placeholder="t('feeds.rule_tag_placeholder')"
                class="w-28 px-2 py-1 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
              />
            </label>
          </div>

          <select
            v-model="rule.sourceIds"
            multiple
            size="3"
            class="w-full px-2 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark"
          >
            <option v-for="source in sources" :key="source.id" :value="source.id">{{ source.title || source.url }}</option>
          </select>
          <p v-if="rule.sourceIds.length === 0" class="text-xs text-gray-400">{{ t('feeds.rule_scope_all') }}</p>
        </div>

        <button
          @click="addRule"
          class="w-full flex items-center justify-center gap-2 py-2.5 rounded-xl border border-dashed border-border dark:border-border-dark text-sm text-gray-500 hover:text-orange-500 hover:border-orange-400 transition-colors"
        >
          <Plus class="w-4 h-4" />
          {{ t('feeds.rule_add') }}
        </button>
      </div>

      <div class="flex items-center gap-2 px-6 py-4 border-t border-gray-200 dark:border-[#2c2c2c]">
        <button
          @click="applyNow"
          :disabled="applying"
          class="px-3 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors disabled:opacity-50"
          :title="t('feeds.rules_apply_hint')"
        >
          <Loader2 v-if="applying" class="w-4 h-4 animate-spin" />
          <span v-else>{{ t('feeds.rules_apply_now') }}</span>
        </button>
        <span v-if="appliedCount !== null" class="text-xs text-gray-400">{{ t('feeds.rules_applied', { count: appliedCount }) }}</span>

        <span class="flex-1"></span>
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          {{ t('feeds.cancel') }}
        </button>
        <button @click="save" class="px-4 py-2 rounded-xl bg-orange-500 text-white text-sm font-medium hover:bg-orange-600 transition-colors shadow-sm">
          {{ t('feeds.save') }}
        </button>
      </div>
    </div>
  </div>
</template>
