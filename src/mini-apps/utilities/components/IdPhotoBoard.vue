<script setup lang="ts">
/**
 * The ID photo tool's middle: the picture with the cut to drag (its shape is
 * the photo's size), and beside it the photo as it will come out — at its
 * size, on its new background.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, ScanFace } from 'lucide-vue-next';
import CropBox from './CropBox.vue';
import { idPhotoCanvas } from '../canvas';
import { idPixels } from '../photo';
import { loadImage } from '../render';
import { useWorkbench, type Item } from '../useWorkbench';

const props = defineProps<{ item: Item }>();
const { t } = useI18n();
const wb = useWorkbench();
const p = wb.options.idphoto;

const recolour = computed(() => p.backdrop !== 'keep' && !!wb.vision.value?.subject);
const result = ref<string | null>(null);
const making = ref(false);
const finding = ref(false);

let serial = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => [props.item.input.id, p.rect?.x, p.rect?.y, p.rect?.width, p.size, p.backdrop],
  () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const rect = p.rect;
      if (!rect) return;
      const mine = ++serial;
      making.value = true;
      try {
        const [img, mask] = await Promise.all([loadImage(props.item.input.path), recolour.value ? wb.maskFor(props.item).catch(() => undefined) : undefined]);
        const [w, h] = idPixels(p.size);
        const canvas = idPhotoCanvas(img, rect, [w, h], recolour.value ? p.backdrop : null, mask);
        const url = canvas.toDataURL('image/jpeg', 0.9);
        canvas.width = canvas.height = 0;
        if (mine === serial) result.value = url;
      } finally {
        if (mine === serial) making.value = false;
      }
    }, 120);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(timer));

async function findFace() {
  finding.value = true;
  await wb.frameIdPhoto(props.item);
  finding.value = false;
}

const [w, h] = [computed(() => idPixels(p.size)[0]), computed(() => idPixels(p.size)[1])];
</script>

<template>
  <div class="flex flex-wrap gap-6 items-start">
    <div class="flex flex-col gap-2">
      <CropBox v-if="item.width && p.rect" :path="item.input.path" :width="item.width" :height="item.height!" target="idphoto" />
      <div v-else class="w-[480px] h-[460px] bg-surface-hover dark:bg-surface-hover-dark animate-pulse rounded-sm" aria-hidden="true" />
      <button v-if="wb.vision.value" type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] self-start" :disabled="finding" @click="findFace">
        <Loader2 v-if="finding" class="w-4 h-4 animate-spin" aria-hidden="true" /><ScanFace v-else class="w-4 h-4" aria-hidden="true" />{{ t('utilities.idphoto.find_face') }}
      </button>
    </div>
    <figure class="m-0 flex flex-col gap-2 items-start">
      <div class="relative border border-border dark:border-border-dark shadow-sm bg-white" :style="{ width: `${w / 2.4}px`, height: `${h / 2.4}px` }">
        <img v-if="result" :src="result" :alt="t('utilities.idphoto.result')" class="block w-full h-full" />
        <Loader2 v-if="making" class="absolute top-2 right-2 w-4 h-4 animate-spin text-muted" aria-hidden="true" />
      </div>
      <figcaption class="text-xs text-muted dark:text-muted-dark tabular-nums">{{ t('utilities.idphoto.caption', { w, h }) }}</figcaption>
    </figure>
  </div>
</template>
