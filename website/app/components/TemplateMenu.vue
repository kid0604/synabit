<script setup lang="ts">
const items = [
  {
    label: 'Home',
    to: '/'
  },
  {
    label: 'Pricing',
    to: '/pricing'
  },
  {
    label: 'Docs',
    to: '/docs'
  }
]
</script>

<template>
  <UNavigationMenu :items="items" class="hidden md:flex" />
</template>
