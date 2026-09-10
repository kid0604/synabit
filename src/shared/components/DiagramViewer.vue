<script setup lang="ts">
/**
 * A diagram, big enough to read.
 *
 * # Why a viewer and not just a bigger picture
 *
 * A Mermaid flowchart drawn for a question like *"the architecture of Splunk"*
 * comes back a thousand pixels wide with four subgraphs in it. A chat bubble is
 * four hundred, and `max-width: 100%` on the SVG dutifully scales the whole
 * thing down until every label is two pixels tall. Nothing was broken — it was
 * legible in exactly the way a map folded to the size of a stamp is legible.
 *
 * `overflow-x: auto` on the container never helped, because the SVG shrank
 * rather than overflowed. So the bubble keeps its tidy thumbnail and this is
 * where the diagram is actually looked at.
 *
 * # Why zoom and pan rather than one bigger size
 *
 * Because there is no one size. A four-node flowchart wants to be shown whole;
 * a sequence diagram with twenty participants wants to be read left to right at
 * full size. Opening at "whole, but never magnified past natural size" covers
 * the first, and the wheel covers the second.
 */
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue';
import { X, ZoomIn, ZoomOut, Maximize2 } from 'lucide-vue-next';

const props = defineProps<{
  /** The rendered SVG, as markup. `null` when nothing is open. */
  svg: string | null;
}>();

const emit = defineEmits<{ close: [] }>();

/** How far out and in it is worth going. Below a fifth it is a thumbnail
 *  again; above eight the strokes are wider than the labels. */
const SMALLEST = 0.2;
const LARGEST = 8;

const scale = ref(1);
const x = ref(0);
const y = ref(0);

const stage = ref<HTMLElement | null>(null);
const art = ref<HTMLElement | null>(null);

/**
 * The size the diagram was drawn at, in its own coordinates.
 *
 * Read off the `viewBox` attribute rather than the DOM property, because
 * `SVGSVGElement.viewBox` is one of the things a test environment does not
 * implement, and a viewer that only works in a browser is a viewer nothing can
 * check.
 */
const drawnSize = (svgEl: SVGElement): { w: number; h: number } | null => {
  const parts = (svgEl.getAttribute('viewBox') ?? '').split(/[\s,]+/).map(Number);
  if (parts.length !== 4 || parts.some(n => !Number.isFinite(n))) return null;
  const [, , w, h] = parts;
  return w > 0 && h > 0 ? { w, h } : null;
};

/**
 * Give the picture a size of its own before anything tries to measure it.
 *
 * # The bug this is
 *
 * Mermaid's `calculateSvgSizeAttrs`, when `useMaxWidth` is on — and it is, by
 * default — sets exactly three things:
 *
 * ```js
 * attrs.set("width", "100%");
 * attrs.set("style", `max-width: ${width}px;`);
 * // ...and later: svgElem.attr("viewBox", vBox);
 * ```
 *
 * No height at all, and the real size only in the `viewBox`. In a chat bubble
 * the `max-width` is the one thing giving it a size. Taking that off — which
 * this viewer must, or the drawing is scaled down and then straight back up —
 * leaves `width: 100%` inside a wrapper that is itself sized to fit its
 * contents. A box whose width depends on its content whose width depends on the
 * box resolves to **zero**: the viewer opened onto an empty stage with the
 * controls still cheerfully reading 100%.
 *
 * So the `viewBox` becomes an explicit width and height, and every measurement
 * after this has something to measure.
 */
const settle = (svgEl: SVGElement) => {
  svgEl.style.maxWidth = 'none';
  const size = drawnSize(svgEl);
  if (!size) return;
  svgEl.setAttribute('width', String(size.w));
  svgEl.setAttribute('height', String(size.h));
};

/**
 * Open at a size that shows the whole thing, and never magnified past its own.
 *
 * A diagram smaller than the window is drawn at natural size — scaling a
 * four-node flowchart up to fill a monitor makes it look like a poster and
 * reads no better. A larger one is fitted, which is the whole reason somebody
 * opened this.
 */
const fit = async () => {
  scale.value = 1;
  x.value = 0;
  y.value = 0;
  await nextTick();

  const svgEl = art.value?.querySelector('svg');
  const room = stage.value;
  if (!svgEl || !room) return;

  settle(svgEl);

  // Measured where possible, and taken from the `viewBox` where nothing can be
  // measured — a hidden or not-yet-laid-out element reports zero, and dividing
  // by it would send the diagram to infinity.
  const box = svgEl.getBoundingClientRect();
  const drawn = box.width && box.height ? { w: box.width, h: box.height } : drawnSize(svgEl);
  if (!drawn) return;

  const room_ = room.getBoundingClientRect();
  if (!room_.width || !room_.height) return;

  scale.value = Math.min(1, room_.width / drawn.w, room_.height / drawn.h);
};

const by = (factor: number) => {
  scale.value = Math.min(LARGEST, Math.max(SMALLEST, scale.value * factor));
};

/**
 * Zoom towards the pointer, not towards the middle.
 *
 * Zooming to the centre means every close look is followed by a drag to put
 * back what the zoom moved away. The pointer is already on the part being
 * looked at; keeping that point still is what makes a wheel feel like a
 * magnifying glass rather than a slider.
 */
const onWheel = (e: WheelEvent) => {
  e.preventDefault();
  const room = stage.value?.getBoundingClientRect();
  if (!room) return;

  const was = scale.value;
  const now = Math.min(LARGEST, Math.max(SMALLEST, was * (e.deltaY < 0 ? 1.12 : 1 / 1.12)));
  if (now === was) return;

  // Where the pointer is, measured from the middle of the stage, which is where
  // the artwork is anchored.
  const px = e.clientX - room.left - room.width / 2;
  const py = e.clientY - room.top - room.height / 2;

  x.value = px - (px - x.value) * (now / was);
  y.value = py - (py - y.value) * (now / was);
  scale.value = now;
};

const dragging = ref(false);
let from = { x: 0, y: 0 };

const onDown = (e: MouseEvent) => {
  dragging.value = true;
  from = { x: e.clientX - x.value, y: e.clientY - y.value };
};
const onMove = (e: MouseEvent) => {
  if (!dragging.value) return;
  x.value = e.clientX - from.x;
  y.value = e.clientY - from.y;
};
const onUp = () => { dragging.value = false; };

const onKey = (e: KeyboardEvent) => {
  if (!props.svg) return;
  if (e.key === 'Escape') emit('close');
  if (e.key === '0') fit();
  if (e.key === '+' || e.key === '=') by(1.25);
  if (e.key === '-') by(1 / 1.25);
};

// A new diagram is a new fit. Opening the same one twice should not remember
// where the last person left it.
watch(() => props.svg, svg => { if (svg) fit(); });

onMounted(() => {
  // Mounted with one already open. The watcher above only fires on a *change*,
  // so without this a viewer created around a diagram would never be measured —
  // which is how the tests mount it, and how a future caller might.
  if (props.svg) fit();

  window.addEventListener('keydown', onKey);
  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', onUp);
});
onUnmounted(() => {
  window.removeEventListener('keydown', onKey);
  window.removeEventListener('mousemove', onMove);
  window.removeEventListener('mouseup', onUp);
});
</script>

<template>
  <Teleport to="body">
    <Transition
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="svg"
        class="fixed inset-0 z-[70] bg-black/85 backdrop-blur-sm flex flex-col"
        @click.self="emit('close')"
      >
        <!-- The controls, and the way out. Always visible rather than on
             hover: a control somebody has to find by waving the mouse is a
             control somebody in a hurry does not have. -->
        <div class="shrink-0 flex items-center gap-1 p-3" @click.stop>
          <button
            class="w-8 h-8 rounded-lg flex items-center justify-center text-white/70
                   hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            :title="$t('syn.diagram_zoom_out')"
            @click="by(1 / 1.25)"
          >
            <ZoomOut class="w-4 h-4" />
          </button>
          <button
            class="min-w-14 h-8 px-2 rounded-lg text-xs font-mono text-white/70
                   hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            :title="$t('syn.diagram_fit')"
            @click="fit()"
          >{{ Math.round(scale * 100) }}%</button>
          <button
            class="w-8 h-8 rounded-lg flex items-center justify-center text-white/70
                   hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            :title="$t('syn.diagram_zoom_in')"
            @click="by(1.25)"
          >
            <ZoomIn class="w-4 h-4" />
          </button>
          <button
            class="w-8 h-8 rounded-lg flex items-center justify-center text-white/70
                   hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            :title="$t('syn.diagram_fit')"
            @click="fit()"
          >
            <Maximize2 class="w-4 h-4" />
          </button>

          <span class="ml-3 text-[11px] text-white/40 select-none">
            {{ $t('syn.diagram_hint') }}
          </span>

          <button
            class="ml-auto w-8 h-8 rounded-lg flex items-center justify-center text-white/70
                   hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            :title="$t('syn.diagram_close')"
            @click="emit('close')"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <!--
          The stage. Clicking the backdrop closes; clicking the artwork does
          not, because a click is also how a drag starts and closing on the end
          of a pan would make the thing impossible to move.
        -->
        <div
          ref="stage"
          class="flex-1 min-h-0 overflow-hidden flex items-center justify-center"
          :class="dragging ? 'cursor-grabbing' : 'cursor-grab'"
          @click.self="emit('close')"
          @wheel="onWheel"
          @mousedown.prevent="onDown"
        >
          <div
            ref="art"
            class="diagram-art origin-center"
            :style="{ transform: `translate(${x}px, ${y}px) scale(${scale})` }"
            v-html="svg"
          />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/*
  Mermaid writes `max-width` into the SVG's own style attribute, which is what
  keeps it inside a chat bubble. In here that is the one thing not wanted: the
  wrapper's transform decides the size, and a max-width fights it by scaling the
  drawing down first and then having it scaled up again — soft edges for no
  reason.
*/
.diagram-art :deep(svg) {
  /*
    `max-width` only. The width and height are set in script from the
    `viewBox`, because `width: auto` on an SVG that says `width="100%"` inside a
    shrink-to-fit wrapper resolves to nothing at all — see `settle`.
  */
  max-width: none !important;
  display: block;
}
</style>
