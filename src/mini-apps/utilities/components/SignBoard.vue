<script setup lang="ts">
/**
 * Sign and fill's page: what has been placed on it, to move, size and pick.
 * What to place, and the words of a picked line, are in the options panel.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import BoxLayer from './BoxLayer.vue';
import PageBoard from './PageBoard.vue';
import { textWidth, type Box, type Mark } from '../marks';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const s = wb.options.sign;
const pdf = computed(() => wb.firstPdf.value!);

const here = computed(() => s.marks.filter((m) => m.page === s.page));
const marked = computed(() => new Set(s.marks.map((m) => m.page)));

function change(id: string, box: Box) {
  const m = s.marks.find((x) => x.id === id);
  if (!m) return;
  Object.assign(m, box);
  // A line of text is as wide as its words at its height.
  if (m.kind === 'text' || m.kind === 'date') m.w = Math.min(1 - m.x, textWidth(m.text, m.h, m.aspect));
}

function remove(id: string) {
  s.marks = s.marks.filter((m) => m.id !== id);
  if (s.selected === id) s.selected = null;
}

const keepShape = (m: Mark) => m.kind !== 'text' && m.kind !== 'date';

function label(m: Mark): string {
  const what = t(`utilities.sign.kind_${m.kind}`);
  return m.text ? `${what}: ${m.text}` : what;
}
</script>

<template>
  <!-- The page's shape is kept for what is placed on it next. -->
  <PageBoard v-model:page="s.page" :path="pdf.input.path" :page-count="pdf.pages ?? 1" :marked="marked" @shape="s.aspect = $event.aspect">
    <template #default="{ width, height }">
      <BoxLayer
        :items="here"
        :selected="s.selected"
        :width="width"
        :height="height"
        :keep-shape="keepShape"
        :label="label"
        @select="s.selected = $event"
        @change="change"
        @remove="remove"
      >
        <template #default="{ item }">
          <img v-if="item.kind === 'signature' && item.picture" :src="item.picture" alt="" class="w-full h-full pointer-events-none" draggable="false" />
          <svg v-else-if="item.kind === 'check' || item.kind === 'cross'" viewBox="0 0 100 100" class="w-full h-full pointer-events-none" aria-hidden="true">
            <path v-if="item.kind === 'check'" d="M15 55 L42 80 L87 20" fill="none" :stroke="item.color" stroke-width="13" stroke-linecap="round" stroke-linejoin="round" />
            <path v-else d="M20 20 L80 80 M80 20 L20 80" fill="none" :stroke="item.color" stroke-width="13" stroke-linecap="round" />
          </svg>
          <span
            v-else
            class="block w-full h-full whitespace-nowrap overflow-hidden pointer-events-none leading-none"
            :style="{ color: item.color, fontSize: `${item.h * height * 0.78}px`, lineHeight: `${item.h * height}px`, paddingLeft: `${item.h * height * 0.06}px` }"
          >{{ item.text || t('utilities.sign.empty_text') }}</span>
        </template>
      </BoxLayer>
    </template>
  </PageBoard>
</template>
