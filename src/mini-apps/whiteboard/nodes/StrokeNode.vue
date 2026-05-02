<script setup lang="ts">
const props = defineProps<{
  id: string;
  data: {
    svgPath: string;
    color: string;
    size: number;
  };
}>();
</script>

<template>
  <div class="wb-stroke-node">
    <svg class="overflow-visible" width="1" height="1">
      <path
        :d="data.svgPath"
        :fill="data.color"
        :opacity="data.opacity ?? 0.85"
        stroke="none"
      />
    </svg>
  </div>
</template>

<style scoped>
.wb-stroke-node {
  pointer-events: none;
}
</style>
