<script setup lang="ts">
/**
 * Redact's page: drag to draw a box over what should go; a box can be moved,
 * sized and taken off again. Boxes found by a search land here too.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import BoxLayer from './BoxLayer.vue';
import PageBoard from './PageBoard.vue';
import { newId, type Box, type Hidden } from '../marks';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();
const r = wb.options.redact;
const pdf = computed(() => wb.firstPdf.value!);

const here = computed(() => r.boxes.filter((b) => b.page === r.page));
const marked = computed(() => new Set(r.boxes.map((b) => b.page)));

function create(box: Box) {
  const made: Hidden = { ...box, id: newId('r'), page: r.page };
  r.boxes.push(made);
  r.selected = made.id;
}

function change(id: string, box: Box) {
  const b = r.boxes.find((x) => x.id === id);
  if (b) Object.assign(b, box);
}

function remove(id: string) {
  r.boxes = r.boxes.filter((b) => b.id !== id);
  if (r.selected === id) r.selected = null;
}
</script>

<template>
  <div class="flex flex-col gap-2">
    <p class="m-0 text-xs text-muted dark:text-muted-dark">{{ t('utilities.redact.draw_hint') }}</p>
    <PageBoard v-model:page="r.page" :path="pdf.input.path" :page-count="pdf.pages ?? 1" :marked="marked">
      <template #default="{ width, height }">
        <BoxLayer
          :items="here"
          :selected="r.selected"
          :width="width"
          :height="height"
          :label="() => t('utilities.redact.box')"
          draw
          @select="r.selected = $event"
          @create="create"
          @change="change"
          @remove="remove"
        >
          <template #default>
            <span class="block w-full h-full bg-black/85" />
          </template>
        </BoxLayer>
      </template>
    </PageBoard>
  </div>
</template>
