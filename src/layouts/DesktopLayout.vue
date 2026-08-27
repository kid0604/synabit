<script setup lang="ts">
// Desktop Layout: Sidebar on the left, Content on the right.
</script>

<template>
  <!--
    No `banner` slot here, deliberately. App.vue passes one, and a slot a
    component does not render is dropped in silence — which is the correct
    outcome: the notices that use it are mobile-only, and restructuring this
    container to hold a full-width bar it will never show would be changing
    desktop layout for nothing.
  -->
  <div class="flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none">
    <slot name="sidebar" />
    <slot />
    <slot name="modal" />
  </div>
</template>
