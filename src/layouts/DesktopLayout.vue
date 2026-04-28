<script setup lang="ts">
// Desktop Layout: Sidebar on the left, Content on the right.
</script>

<template>
  <div class="flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none">
    <slot name="sidebar" />
    <slot />
    <slot name="modal" />
  </div>
</template>
