<script setup lang="ts">
/**
 * What kind of value a field holds: text, number, yes/no, date, list.
 *
 * A row of pills rather than a `<select>`, for the reason the type chooser
 * stopped being a `<datalist>`. WKWebView draws native form controls with its
 * own chrome and ignores the stylesheet, so a select arrives wearing macOS's
 * double chevron in the middle of a form that looks nothing like macOS — the
 * one control on the page the app did not draw.
 *
 * And a dropdown was the wrong shape anyway. There are five options, each a
 * word long; hiding them behind a click to save eighty pixels trades something
 * you can read for something you have to open.
 *
 * Only used for a field with no value yet. Once one exists `kindOf` reads the
 * kind from the value, because a declaration that disagrees with the file is
 * wrong about the file.
 */
import { useI18n } from 'vue-i18n';
import { DECLARABLE_KINDS, type FieldKind } from '../fieldValue';

const model = defineModel<FieldKind>({ required: true });

const { t } = useI18n();
</script>

<template>
  <div
    class="flex items-center gap-0.5 p-0.5 rounded-lg flex-none
           bg-gray-100 dark:bg-white/5"
    role="radiogroup"
  >
    <button
      v-for="kind in DECLARABLE_KINDS"
      :key="kind"
      type="button"
      role="radio"
      :aria-checked="model === kind"
      @click="model = kind"
      class="px-2 py-1 rounded-md text-[11px] whitespace-nowrap cursor-pointer transition-colors"
      :class="model === kind
        ? 'bg-white dark:bg-[#2c2c2c] text-[#1c1c1e] dark:text-[#f4f4f5] shadow-sm'
        : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200'"
    >
      {{ t(`things.kind_${kind}`) }}
    </button>
  </div>
</template>
