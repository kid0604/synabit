<script setup lang="ts">
/**
 * The last swatch in a palette: any colour, through the system's own picker.
 *
 * The presets cover most of what a board needs; a brand colour, or matching a
 * picture, needs the rest. The native control rather than a picker of our own
 * — every WebView this app runs in has one, and it is the one the user knows.
 */
const props = defineProps<{
  /** The colour the item has now; the swatch shows as chosen when it is not a preset. */
  value?: string;
  presets: string[];
  label: string;
}>();

const emit = defineEmits<{ (e: 'pick', color: string): void }>();

const isCustom = () => !!props.value && !props.presets.includes(props.value);
// The picker only takes six-digit hex; anything else starts it from black.
const pickerValue = () => (/^#[0-9a-f]{6}$/i.test(props.value ?? '') ? props.value : '#000000');
</script>

<template>
  <label
    class="wb-custom-swatch"
    :class="{ active: isCustom() }"
    :style="isCustom() ? { '--sw-color': value } : {}"
    :title="label"
  >
    <input
      type="color"
      class="wb-custom-swatch__input"
      :value="pickerValue()"
      :aria-label="label"
      @input="emit('pick', ($event.target as HTMLInputElement).value)"
    />
  </label>
</template>

<style scoped>
.wb-custom-swatch {
  position: relative;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  cursor: pointer;
  /* A colour wheel until a colour of its own is chosen. */
  background: conic-gradient(#ef4444, #f59e0b, #10b981, #06b6d4, #3b82f6, #7c3aed, #ec4899, #ef4444);
  box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.12);
}
.wb-custom-swatch.active {
  background: var(--sw-color);
  box-shadow: 0 0 0 2px var(--color-surface, #fff), 0 0 0 4px var(--color-accent);
}
.wb-custom-swatch:focus-within {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}
.wb-custom-swatch__input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0;
  cursor: pointer;
  border: 0;
  padding: 0;
}
</style>
