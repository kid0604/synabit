<script setup lang="ts">
import ConnectCard from './ConnectCard.vue';

/**
 * A connector Syn drafted or found, in the conversation where it asked for
 * it: the connect card, and the run told whether it was connected. See
 * `ConnectCard` and `connector::authoring`.
 */
defineProps<{ vaultPath: string; connectorId: string; busy?: boolean; error?: string | null }>();
const emit = defineEmits<{ answer: [approved: boolean] }>();
</script>

<template>
  <div data-test="connector-proposal" class="space-y-1">
    <ConnectCard :vault-path="vaultPath" :connector-id="connectorId" @done="emit('answer', $event)" />
    <p v-if="error" class="text-xs text-red-600 dark:text-red-400">{{ error }}</p>
  </div>
</template>
