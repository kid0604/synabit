<template>
  <img
    :src="synAvatar"
    alt="Syn"
    class="rounded-full object-cover"
    :style="{ width: typeof size === 'number' ? size + 'px' : size, height: typeof size === 'number' ? size + 'px' : size }"
  />
</template>

<script setup lang="ts">
import synAvatar from '../../assets/syn-avatar.jpg';

withDefaults(defineProps<{
  size?: number | string;
}>(), {
  size: 24,
});
</script>
