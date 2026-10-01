<script setup lang="ts">
/**
 * A run's own plan, as a list somebody can read at a glance.
 *
 * The same list while the run works and under the answer afterwards, so the
 * plan a person watched being followed is the plan they can come back to.
 * Status is in the icon and, for a screen reader, in words — a colour or a
 * tick alone says nothing to someone who cannot see it.
 */
import { CheckCircle2, Circle, CircleDot } from 'lucide-vue-next';
import type { PlanStep } from '../../mini-apps/messages/types';

defineProps<{ steps: PlanStep[] }>();
</script>

<template>
  <ol class="space-y-1 text-sm">
    <li v-for="(step, i) in steps" :key="i" class="flex items-start gap-2">
      <CheckCircle2 v-if="step.status === 'done'" class="w-4 h-4 mt-0.5 shrink-0 text-emerald-500" aria-hidden="true" />
      <CircleDot v-else-if="step.status === 'doing'" class="w-4 h-4 mt-0.5 shrink-0 text-accent dark:text-accent-dark" aria-hidden="true" />
      <Circle v-else class="w-4 h-4 mt-0.5 shrink-0 text-gray-500 dark:text-gray-400" aria-hidden="true" />
      <span
        :class="{
          'text-gray-500 dark:text-gray-400 line-through decoration-gray-300 dark:decoration-gray-600': step.status === 'done',
          'font-medium text-text dark:text-text-dark': step.status === 'doing',
          'text-gray-600 dark:text-gray-300': step.status === 'todo',
        }"
      >
        {{ step.text }}
        <span class="sr-only">({{ $t(`syn.plan_status_${step.status}`) }})</span>
      </span>
    </li>
  </ol>
</template>
