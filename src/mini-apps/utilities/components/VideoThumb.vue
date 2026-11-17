<script setup lang="ts">
/**
 * One frame of a video, small, made by ffmpeg in a sealed room: the WebView
 * plays MP4 and QuickTime, but not MKV, AVI and the rest, and a frame is the
 * same for all of them. Made once per file and kept while the app is open.
 */
import { onMounted, ref } from 'vue';
import { Film } from 'lucide-vue-next';
import { videoPicture } from '../api';

const props = defineProps<{ input: string; at: number }>();

const url = ref<string | null>(null);
const failed = ref(false);

onMounted(async () => {
  const key = props.input;
  let picture = thumbs.get(key);
  if (!picture) {
    picture = videoPicture(props.input, props.at, 320).then((blob) => URL.createObjectURL(blob));
    thumbs.set(key, picture);
    picture.catch(() => thumbs.delete(key));
  }
  try {
    url.value = await picture;
  } catch {
    failed.value = true;
  }
});
</script>

<script lang="ts">
/** By input id: an input's file never changes, so neither does its frame. */
const thumbs = new Map<string, Promise<string>>();
</script>

<template>
  <img v-if="url" :src="url" alt="" class="w-full aspect-video object-cover rounded-sm border border-border dark:border-border-dark bg-black" />
  <div v-else class="w-full aspect-video rounded-sm bg-surface-hover dark:bg-surface-hover-dark grid place-items-center text-muted dark:text-muted-dark" :class="failed ? '' : 'animate-pulse'">
    <Film class="w-6 h-6" aria-hidden="true" />
  </div>
</template>
