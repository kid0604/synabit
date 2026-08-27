<script setup lang="ts">
import { hours, formatHourAMPM } from '../helpers';

defineProps<{ hourHeight: number }>();
</script>

<template>
    <!--
      Absolutely positioned, not stacked.

      Stacking 24 rows and nudging each up to straddle its line made every
      nudge cumulative: the axis came out 192px shorter than the columns
      beside it, so the labels drifted further from the hour they named the
      further down the day you looked. Positioning each label at its own
      offset inside a box of the same height as a day column cannot drift.

      aria-hidden: the hours label the grid visually; every block carries its
      own time in its accessible name.
    -->
    <div class="relative select-none" :style="{ height: hourHeight * 24 + 'px' }" aria-hidden="true">
        <div v-for="hr in hours" :key="'lbl-' + hr"
             v-show="hr > 0"
             class="absolute right-0 pr-2 -translate-y-1/2 text-[10px] font-medium text-gray-400 whitespace-nowrap"
             :style="{ top: hr * hourHeight + 'px' }">
            {{ formatHourAMPM(hr) }}
        </div>
    </div>
</template>
