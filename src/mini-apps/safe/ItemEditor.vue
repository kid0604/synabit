<script setup lang="ts">
/**
 * Creating or editing an item, in the shared `AppDialog`, which owns Escape,
 * the scrim and where focus goes.
 *
 * # Concealed values are not sent here to be edited
 *
 * An existing password arrives as "unchanged" and goes back as `unchanged`
 * unless the user presses Change. The editor never asks Rust for the current
 * value: editing a title should not put every password of the item into the
 * WebView. Changing one starts from empty or from the generator.
 */
import { computed, nextTick, onMounted, reactive, ref } from 'vue';
import AppDialog from '../../shared/components/AppDialog.vue';
import { useI18n } from 'vue-i18n';
import { Dices, Eye, EyeOff, Plus, X } from 'lucide-vue-next';
import type { EditValue, FieldKind, ItemEdit, ItemKind, ItemView, SafeApi, TotpEdit } from './api';
import { CONCEALED, FIELD_KINDS, kindInfo } from './kinds';
import PasswordGenerator from './PasswordGenerator.vue';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi; item: ItemView | null; kind: ItemKind }>();
const emit = defineEmits<{ (e: 'saved', item: ItemView): void; (e: 'close'): void }>();
const { t } = useI18n();
const explain = useSafeError();

interface DraftField {
  key: number;
  id: string | null;
  label: string;
  kind: FieldKind;
  value: string;
  /** An existing concealed value the user has not chosen to change. */
  keep: boolean;
  shown: boolean;
  generating: boolean;
}

let nextKey = 0;
const kind = ref<ItemKind>(props.item?.kind ?? props.kind);
const title = ref(props.item?.title ?? '');
const notes = ref(props.item?.notes ?? '');
/** `YYYY-MM-DD`, or empty for none; the item keeps Unix seconds. */
const expires = ref(props.item?.expires_at ? new Date(props.item.expires_at * 1000).toISOString().slice(0, 10) : '');
/** An existing setup is never sent here; the choice is keep, remove or replace. */
const hasTotp = ref(!!props.item?.totp);
const totpInput = ref('');
const totpEdit = computed<TotpEdit>(() =>
  totpInput.value.trim() ? { t: 'set', v: totpInput.value.trim() } : props.item?.totp && !hasTotp.value ? { t: 'remove' } : { t: 'unchanged' },
);
const tags = ref((props.item?.tags ?? []).join(', '));
const urls = ref<string[]>(props.item ? props.item.urls.map((u) => u.url) : kindInfo(props.kind).url ? [''] : []);
const fields = reactive<DraftField[]>(
  props.item
    ? props.item.fields.map((f) => ({
        key: nextKey++, id: f.id, label: f.label, kind: f.kind,
        value: f.value ?? '', keep: f.concealed && !f.empty, shown: false, generating: false,
      }))
    : kindInfo(props.kind).fields.map((f) => ({
        key: nextKey++, id: null, label: t(`safe.field.${f.label}`), kind: f.kind,
        value: '', keep: false, shown: false, generating: false,
      })),
);

const isConcealed = (k: FieldKind) => CONCEALED.includes(k);
/** A concealed value made of lines: an SSH private key, or anything pasted with line breaks. */
const isKeyBlock = (f: { label: string; kind: FieldKind; value: string }) =>
  isConcealed(f.kind) && (f.label === 'private_key' || f.value.includes('\n'));

function addField() {
  fields.push({ key: nextKey++, id: null, label: '', kind: 'text', value: '', keep: false, shown: false, generating: false });
}

function useGenerated(f: DraftField, value: string) {
  f.value = value;
  f.keep = false;
  f.shown = true;
  f.generating = false;
}

const busy = ref(false);
const error = ref('');

const edit = computed<ItemEdit>(() => ({
  kind: kind.value,
  title: title.value,
  fields: fields.map((f) => ({
    id: f.id,
    label: f.label,
    kind: f.kind,
    value: (f.keep ? { t: 'unchanged' } : { t: 'set', v: f.value }) as EditValue,
  })),
  urls: urls.value.filter((u) => u.trim()).map((url) => ({ url: url.trim(), match: 'domain' as const })),
  tags: tags.value.split(',').map((s) => s.trim()).filter(Boolean),
  favorite: props.item?.favorite ?? false,
  notes: notes.value,
  totp: totpEdit.value,
  expires_at: expires.value ? Math.floor(new Date(`${expires.value}T00:00:00`).getTime() / 1000) : null,
}));

async function submit() {
  if (busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    const saved = props.item ? await props.api.updateItem(props.item.id, edit.value) : await props.api.createItem(edit.value);
    // Nothing typed here outlives the dialog.
    for (const f of fields) f.value = '';
    emit('saved', saved);
  } catch (e) {
    error.value = explain(e);
  } finally {
    busy.value = false;
  }
}

const titleInput = ref<HTMLInputElement | null>(null);
onMounted(async () => {
  await nextTick();
  titleInput.value?.focus();
});
</script>

<template>
  <AppDialog :show="true" labelledby="safe-editor-title" :initial-focus="() => titleInput" size="md" unstyled panel-class="bg-surface dark:bg-surface-dark text-text dark:text-text-dark rounded-2xl shadow-2xl border border-border dark:border-border-dark flex flex-col overflow-hidden max-h-[calc(100vh-64px)]" @close="emit('close')">
    <form class="flex flex-col min-h-0" @submit.prevent="submit">
      <header class="px-5 pt-5 pb-3 flex items-center gap-3">
        <component :is="kindInfo(kind).icon" class="w-5 h-5 text-accent" />
        <h2 id="safe-editor-title" class="text-lg font-semibold flex-1">
          {{ item ? t('safe.editor.edit_title') : t('safe.editor.new_title', { kind: t(`safe.kind.${kind}`) }) }}
        </h2>
        <button type="button" class="btn-icon -mr-2" :aria-label="t('safe.editor.cancel')" :title="t('safe.editor.cancel')" @click="emit('close')">
          <X class="w-4 h-4" />
        </button>
      </header>

      <div class="px-5 pb-4 space-y-4 overflow-y-auto">
        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.title') }}</span>
          <input ref="titleInput" v-model="title" :placeholder="t('safe.editor.title_placeholder')" class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent" />
        </label>

        <div v-for="(f, i) in fields" :key="f.key" class="space-y-1.5">
          <div class="flex items-center gap-2">
            <input v-model="f.label" :placeholder="t('safe.editor.label')" :aria-label="t('safe.editor.label')" class="flex-1 min-w-0 bg-transparent text-xs font-medium text-text-secondary dark:text-text-secondary-dark focus:outline-none" />
            <select v-model="f.kind" class="text-xs bg-transparent text-text-tertiary dark:text-text-tertiary-dark focus:outline-none" :disabled="f.keep">
              <option v-for="k in FIELD_KINDS" :key="k" :value="k">{{ t(`safe.field_kind.${k}`) }}</option>
            </select>
            <button type="button" class="inline-flex items-center justify-center min-w-6 min-h-6 p-1 rounded hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-tertiary dark:text-text-tertiary-dark" :aria-label="t('safe.editor.remove_field')" @click="fields.splice(i, 1)">
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <div v-if="f.keep" class="flex items-center gap-2">
            <span class="flex-1 px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-dashed border-border dark:border-border-dark text-sm text-text-tertiary dark:text-text-tertiary-dark">
              •••••••• · {{ t('safe.editor.unchanged') }}
            </span>
            <button type="button" class="px-3 py-2 text-sm rounded-lg border border-border dark:border-border-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="f.keep = false">
              {{ t('safe.editor.change') }}
            </button>
          </div>
          <div v-else class="flex items-start gap-2">
            <textarea
              v-if="f.kind === 'multiline' || (isKeyBlock(f) && f.shown)"
              v-model="f.value" :rows="isKeyBlock(f) ? 8 : 3"
              autocomplete="off" spellcheck="false" autocapitalize="off"
              class="flex-1 px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
              :class="{ 'text-xs': isKeyBlock(f) }"
            />
            <!-- A private key is lines: a one-line field would join them and the
                 key would no longer read. Hidden until asked for, like any
                 concealed value, and typed or pasted where the lines survive. -->
            <button
              v-else-if="isKeyBlock(f)"
              type="button"
              class="flex-1 px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-left text-sm text-text-tertiary dark:text-text-tertiary-dark"
              @click="f.shown = true"
            >
              {{ f.value ? t('safe.editor.key_hidden', { n: f.value.split('\n').length }) : t('safe.editor.key_paste') }}
            </button>
            <div v-else class="relative flex-1">
              <input
                v-model="f.value"
                :type="isConcealed(f.kind) && !f.shown ? 'password' : 'text'"
                autocomplete="off" spellcheck="false" autocapitalize="off"
                class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent"
                :class="{ 'font-mono pr-9': isConcealed(f.kind) }"
              />
              <button v-if="isConcealed(f.kind) && !isKeyBlock(f)" type="button" class="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-text-tertiary dark:text-text-tertiary-dark" :aria-label="f.shown ? t('safe.detail.hide') : t('safe.detail.reveal')" :title="f.shown ? t('safe.detail.hide') : t('safe.detail.reveal')" @click="f.shown = !f.shown">
                <EyeOff v-if="f.shown" class="w-4 h-4" /><Eye v-else class="w-4 h-4" />
              </button>
            </div>
            <button v-if="isConcealed(f.kind)" type="button" class="p-2 rounded-lg border border-border dark:border-border-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('safe.editor.generate')" :title="t('safe.editor.generate')" @click="f.generating = !f.generating">
              <Dices class="w-4 h-4" />
            </button>
          </div>
          <PasswordGenerator v-if="f.generating && !f.keep" :api="api" @use="useGenerated(f, $event)" />
        </div>

        <button type="button" class="inline-flex items-center gap-1.5 min-h-6 text-sm text-accent hover:underline" @click="addField">
          <Plus class="w-4 h-4" /> {{ t('safe.editor.add_field') }}
        </button>

        <div class="space-y-1.5">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.website') }}</span>
          <div v-for="(_, i) in urls" :key="i" class="flex gap-2">
            <input v-model="urls[i]" type="url" inputmode="url" placeholder="https://" spellcheck="false" class="flex-1 px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent" />
            <button type="button" class="p-2 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-tertiary dark:text-text-tertiary-dark" :aria-label="t('safe.editor.remove_field')" @click="urls.splice(i, 1)">
              <X class="w-4 h-4" />
            </button>
          </div>
          <button type="button" class="inline-flex items-center gap-1.5 min-h-6 text-sm text-accent hover:underline" @click="urls.push('')">
            <Plus class="w-4 h-4" /> {{ t('safe.editor.add_website') }}
          </button>
        </div>

        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.tags') }}</span>
          <input v-model="tags" :placeholder="t('safe.editor.tags_placeholder')" class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent" />
        </label>

        <div class="space-y-1.5">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.totp.label') }}</span>
          <div v-if="hasTotp && !totpInput" class="flex items-center gap-2">
            <span class="flex-1 px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-dashed border-border dark:border-border-dark text-sm text-text-tertiary dark:text-text-tertiary-dark">
              {{ t('safe.totp.configured') }}
            </span>
            <button type="button" class="px-3 py-2 text-sm rounded-lg text-danger hover:bg-danger/10" @click="hasTotp = false">{{ t('safe.totp.remove') }}</button>
          </div>
          <input
            v-else
            v-model="totpInput"
            type="password"
            :placeholder="t('safe.totp.placeholder')"
            :aria-label="t('safe.totp.label')"
            autocomplete="off" spellcheck="false" autocapitalize="off"
            class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
          />
        </div>

        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.expires') }}</span>
          <input v-model="expires" type="date" class="px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm focus:outline-none focus:ring-2 focus:ring-accent" />
        </label>

        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.notes') }}</span>
          <textarea v-model="notes" rows="3" class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm focus:outline-none focus:ring-2 focus:ring-accent" />
        </label>

        <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
      </div>

      <footer class="px-5 py-4 flex justify-end gap-2 border-t border-border-subtle dark:border-border-subtle-dark">
        <button type="button" class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="emit('close')">
          {{ t('safe.editor.cancel') }}
        </button>
        <button type="submit" :disabled="busy || !title.trim()" class="btn-primary">
          {{ t('safe.editor.save') }}
        </button>
      </footer>
    </form>
  </AppDialog>
</template>
