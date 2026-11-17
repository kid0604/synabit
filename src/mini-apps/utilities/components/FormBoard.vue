<script setup lang="ts">
/**
 * Fill form's page: each box of the form, filled in where it is on the page —
 * a text field as a field, a checkbox as a checkbox, a list as a list. Boxes
 * of one name are one value, as in the PDF.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import PageBoard from './PageBoard.vue';
import type { FormField } from '../render';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const ff = wb.options.fillform;
const pdf = computed(() => wb.firstPdf.value!);

const here = computed(() => ff.fields.filter((f) => f.page === ff.page));
const pages = computed(() => new Set(ff.fields.map((f) => f.page)));

function place(f: FormField, height: number) {
  const px = f.box.h * height;
  return {
    left: `${f.box.x * 100}%`,
    top: `${f.box.y * 100}%`,
    width: `${f.box.w * 100}%`,
    height: `${f.box.h * 100}%`,
    fontSize: `${Math.max(12, Math.min(15, f.multiline ? 13 : px * 0.62))}px`,
  };
}

/** What a box is called to a screen reader: the last part of its name, readable. */
function label(f: FormField): string {
  const last = f.name.split('.').pop() ?? f.name;
  return last.replace(/\[\d+\]$/, '').replace(/[_-]+/g, ' ').trim() || t('utilities.form.unnamed');
}

function setText(f: FormField, e: Event) {
  ff.values[f.name] = (e.target as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement).value;
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <PageBoard v-model:page="ff.page" :path="pdf.input.path" :page-count="pdf.pages ?? 1" :marked="pages">
      <template #default="{ height }">
        <div class="absolute inset-0">
          <template v-for="f in here" :key="f.id">
            <input
              v-if="f.type === 'text' && !f.multiline"
              :value="ff.values[f.name] ?? ''"
              :maxlength="f.maxLength"
              :readonly="f.readOnly"
              :aria-label="label(f)"
              :title="label(f)"
              class="utility-form-box absolute px-1 leading-none"
              :style="place(f, height)"
              @input="setText(f, $event)"
            />
            <textarea
              v-else-if="f.type === 'text'"
              :value="ff.values[f.name] ?? ''"
              :maxlength="f.maxLength"
              :readonly="f.readOnly"
              :aria-label="label(f)"
              :title="label(f)"
              class="utility-form-box absolute px-1 resize-none leading-tight"
              :style="place(f, height)"
              @input="setText(f, $event)"
            />
            <span v-else-if="f.type === 'checkbox' || f.type === 'radiobutton'" class="utility-form-box absolute grid place-items-center" :style="place(f, height)">
              <input
                v-if="f.type === 'checkbox'"
                type="checkbox"
                class="w-[80%] h-[80%] max-w-4 max-h-4 accent-accent"
                :checked="ff.values[f.name] === 'on'"
                :disabled="f.readOnly"
                :aria-label="label(f)"
                @change="ff.values[f.name] = ($event.target as HTMLInputElement).checked ? 'on' : ''"
              />
              <input
                v-else
                type="radio"
                class="w-[80%] h-[80%] max-w-4 max-h-4 accent-accent"
                :name="`utility-form-${f.name}`"
                :checked="!!f.exportValue && ff.values[f.name] === f.exportValue"
                :disabled="f.readOnly"
                :aria-label="`${label(f)}: ${f.exportValue ?? ''}`"
                @change="ff.values[f.name] = f.exportValue ?? ''"
              />
            </span>
            <select
              v-else
              :value="ff.values[f.name] ?? ''"
              :disabled="f.readOnly"
              :size="f.type === 'listbox' ? Math.max(2, Math.min(6, f.choices?.length ?? 2)) : undefined"
              :aria-label="label(f)"
              class="utility-form-box absolute"
              :style="place(f, height)"
              @change="setText(f, $event)"
            >
              <option v-if="f.type === 'combobox'" value="" />
              <option v-for="c in f.choices ?? []" :key="c.value" :value="c.value">{{ c.label }}</option>
            </select>
          </template>
        </div>
      </template>
    </PageBoard>
  </div>
</template>

<style scoped>
.utility-form-box {
  background: rgb(219 234 254 / 0.55);
  border: 1px solid rgb(59 130 246 / 0.45);
  color: #111827;
  border-radius: 2px;
}
.utility-form-box:focus,
.utility-form-box:focus-within {
  outline: 2px solid rgb(59 130 246);
  outline-offset: 0;
  background: rgb(255 255 255 / 0.9);
}
</style>
