<script setup lang="ts">
/**
 * An article thumbnail, loaded from the app's image cache rather than from
 * the publisher. Renders nothing at all until there is something to render,
 * so a picture that cannot be cached leaves no gap and no broken icon.
 */
import { ref, watch } from 'vue';
import { useImageCache } from '../composables/useImageCache';

// The root element is absent until there is an image, so the class and other
// attributes the caller passes are bound by hand rather than inherited.
defineOptions({ inheritAttrs: false });

const props = defineProps<{ src: string; alt?: string }>();

const { resolveOne } = useImageCache();
const localSrc = ref('');

watch(
  () => props.src,
  async src => {
    localSrc.value = '';
    if (!src) return;
    const resolvedSrc = await resolveOne(src);
    // The card may have been recycled onto another article while we waited.
    if (props.src === src) localSrc.value = resolvedSrc;
  },
  { immediate: true },
);
</script>

<template>
  <img v-if="localSrc" v-bind="$attrs" :src="localSrc" :alt="alt ?? ''" loading="lazy" decoding="async" />
</template>
