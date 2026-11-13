<script setup lang="ts">
/**
 * One tool, one screen (design §5.3): a drop zone until there are files, then
 * the files or pages in the middle and the options on the right, a progress
 * card over the middle while it runs, and the result in its place when done.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Download, ExternalLink, Eye, EyeOff, FolderOpen, Info, Loader2, RefreshCw, TriangleAlert, Upload } from 'lucide-vue-next';
import FileCards from './FileCards.vue';
import PageGrid from './PageGrid.vue';
import ResultPanel from './ResultPanel.vue';
import MarkPreview from './MarkPreview.vue';
import CropBox from './CropBox.vue';
import CutoutPreview from './CutoutPreview.vue';
import * as api from '../api';
import { openUrl } from '@tauri-apps/plugin-opener';
import { COMPRESS_LEVELS } from '../useWorkbench';
import { logger } from '../../../utils/logger';
import { useWorkbench } from '../useWorkbench';
import { NUMBER_FORMATS, availability, cleansInPlace, extensionFor, fits, numberText, outputName, ownFormat, previewMayRefuse, type ToolId } from '../tools';
import type { Job } from '../api';

const emit = defineEmits<{ (e: 'pick'): void; (e: 'chain', tool: ToolId): void; (e: 'discarded'): void }>();

const { t } = useI18n();
const wb = useWorkbench();
const tool = computed(() => wb.current.value!);

const canAdd = computed(() => tool.value.max === undefined || wb.usable.value.length < tool.value.max);
const tooFew = computed(() => wb.phase.value === 'loaded' && !fits(tool.value, wb.usable.value.length) && wb.usable.value.length < tool.value.min);

const percent = computed(() => (wb.progress.total ? Math.round((wb.progress.done / wb.progress.total) * 100) : 0));

/** What the run will make, said before it runs. */
const plan = computed(() => {
  const files = wb.usable.value;
  if (!files.length) return '';
  const first = files[0].input.name;
  switch (tool.value.id) {
    case 'split':
      return t('utilities.options.makes_files', { n: wb.splitPlan.value.parts.length }, wb.splitPlan.value.parts.length);
    case 'rotate':
      return files.length > 1 ? t('utilities.options.makes_files', { n: files.length }, files.length) : outputName('rotate', first);
    case 'pdf2img': {
      const n = files.reduce((sum, f) => sum + (f.pages ?? 0), 0);
      return t('utilities.options.makes_images', { n }, n);
    }
    case 'ocr': {
      if (files.length === 1) {
        return [wb.options.ocr.pdf && outputName('ocr', first), wb.options.ocr.text && outputName('pdf2text', first)].filter(Boolean).join(' + ');
      }
      return [wb.options.ocr.pdf && `${files.length} PDF`, wb.options.ocr.text && `${files.length} .txt`].filter(Boolean).join(' + ');
    }
    case 'imgtext':
      return files.length > 1 ? t('utilities.options.makes_texts', { n: files.length }, files.length) : outputName('imgtext', first);
    case 'removebg': {
      const ext = wb.options.removebg.backdrop === 'transparent' ? 'png' : 'jpg';
      return files.length > 1 ? t('utilities.options.makes_images', { n: files.length }, files.length) : outputName('removebg', first, undefined, ext);
    }
    case 'imgcompress':
    case 'imgconvert':
    case 'resize':
    case 'crop':
    case 'exif': {
      if (files.length > 1) return t('utilities.options.makes_images', { n: files.length }, files.length);
      const own = ownFormat(first, wb.webp.value);
      const format =
        tool.value.id === 'imgconvert' ? wb.options.imgconvert.format
        : tool.value.id === 'imgcompress' && wb.options.imgcompress.format !== 'keep' ? wb.options.imgcompress.format
        : own;
      // A JPEG or PNG is cleaned as it is; anything else is redrawn, and named for what it becomes.
      const keepsKind = tool.value.id === 'exif' && cleansInPlace(first);
      return outputName(tool.value.id, first, undefined, keepsKind ? undefined : extensionFor(format));
    }
    default:
      return files.length > 1 && tool.value.id !== 'merge' && tool.value.id !== 'img2pdf'
        ? t('utilities.options.makes_files', { n: files.length }, files.length)
        : outputName(tool.value.id, first);
  }
});

const splitSummary = computed(() =>
  wb.splitPlan.value.parts
    .map((pages) =>
      pages.length === 1
        ? t('utilities.options.page_n', { n: pages[0] })
        : wb.options.split.mode === 'pick'
          ? pages.join(', ')
          : t('utilities.options.pages_range', { from: pages[0], to: pages[pages.length - 1] }),
    )
    .join(' · '),
);

let lastPicked: number | null = null;
function togglePage(page: number, extend: boolean) {
  const picked = new Set(wb.options.split.picked);
  if (extend && lastPicked !== null) {
    const [from, to] = lastPicked < page ? [lastPicked, page] : [page, lastPicked];
    for (let p = from; p <= to; p++) picked.add(p);
  } else if (picked.has(page)) {
    picked.delete(page);
  } else {
    picked.add(page);
  }
  lastPicked = page;
  wb.options.split.picked = picked;
}

function turnPage(index: number, by: number) {
  const slot = wb.options.organize.pages[index];
  slot.rotate = (((slot.rotate + by) % 360) + 360) % 360;
}

function leaveOut(index: number) {
  wb.options.organize.pages.splice(index, 1);
}

function movePage(from: number, to: number) {
  const list = wb.options.organize.pages;
  const [moved] = list.splice(from, 1);
  list.splice(to, 0, moved);
}

function resetOrganize() {
  const pages = wb.firstPdf.value?.pages ?? 0;
  wb.options.organize.pages = Array.from({ length: pages }, (_, i) => ({ page: i + 1, rotate: 0, key: `r${Date.now()}-${i}` }));
}

function onSaved(job: Job) {
  wb.noteSaved(job);
}

const acceptLabel = computed(() => t(`utilities.accepts.${tool.value.accepts}`));

/** Whether Vision can run this tool here; the screen explains when not. */
const avail = computed(() => availability(tool.value, wb.vision.value, wb.office.value));
const hasVietnamese = computed(() => !!wb.vision.value?.languages.includes('vi-VT'));

const checking = ref(false);
async function checkAgain() {
  checking.value = true;
  await wb.askOffice();
  checking.value = false;
}
function openDownload() {
  void openUrl('https://www.libreoffice.org/download/download-libreoffice/').catch(logger.error);
}

const firstImage = computed(() => wb.usable.value.find((i) => i.kind === 'image'));

/** Choose the picture a logo watermark uses. */
async function pickLogo() {
  try {
    const [logo] = await api.pickInputs(['image'], false);
    if (logo) wb.options.watermark.logo = logo;
  } catch (e) {
    logger.error('[utilities] picking a logo failed', e);
  }
}

const showPassword = ref(false);
const mismatch = computed(() => !!wb.options.protect.confirm && wb.options.protect.password !== wb.options.protect.confirm);

function setAspect(aspect: string) {
  wb.options.crop.aspect = aspect;
  wb.resetCrop();
}

/** Images a compression would make, with formats this WebView can write. */
const formats = computed(() => (wb.webp.value ? (['jpeg', 'png', 'webp'] as const) : (['jpeg', 'png'] as const)));
</script>

<template>
  <div class="flex h-full min-h-0">
    <!-- The middle -->
    <div class="relative flex-1 min-w-0">
      <div class="h-full overflow-auto px-6 py-5">
        <div v-if="wb.failure.value" role="alert" class="mb-4 flex items-start gap-2.5 rounded-xl border border-danger/40 bg-danger/5 px-3.5 py-2.5 text-sm text-text dark:text-text-dark">
          <TriangleAlert class="w-4 h-4 mt-0.5 shrink-0 text-danger" aria-hidden="true" />
          <span>{{ t(wb.failure.value.key, wb.failure.value.values ?? {}) }}</span>
        </div>

        <!-- Waiting for something to be installed -->
        <div v-if="avail === 'needs-install' && wb.phase.value !== 'done'" class="max-w-[560px] mx-auto mt-10 rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark p-6">
          <h2 class="flex items-center gap-2.5 text-lg font-semibold m-0 text-text dark:text-text-dark">
            <span class="w-9 h-9 rounded-[9px] grid place-items-center" :class="`utility-tint-${tool.category}`"><component :is="tool.icon" class="w-5 h-5" aria-hidden="true" /></span>
            {{ t(`utilities.setup.${tool.needs}_title`, { tool: t(`utilities.tools.${tool.id}.name`) }) }}
          </h2>
          <p class="text-text-secondary dark:text-text-secondary-dark leading-relaxed mt-3 mb-0">{{ t(`utilities.setup.${tool.needs}_body`) }}</p>
          <p class="mt-3 mb-0 flex gap-2.5 rounded-lg bg-surface-hover dark:bg-surface-hover-dark px-3 py-2.5 text-[13px] text-text-secondary dark:text-text-secondary-dark">
            <Info class="w-4 h-4 shrink-0 mt-0.5" aria-hidden="true" />{{ t('utilities.setup.sealed') }}
          </p>
          <div class="flex flex-wrap gap-2 mt-4">
            <template v-if="tool.needs === 'libreoffice'">
              <button type="button" class="btn-primary" @click="openDownload"><ExternalLink class="w-4 h-4" aria-hidden="true" />{{ t('utilities.setup.download') }}</button>
              <button type="button" class="btn-secondary" :disabled="checking" @click="checkAgain">
                <Loader2 v-if="checking" class="w-4 h-4 animate-spin" aria-hidden="true" /><RefreshCw v-else class="w-4 h-4" aria-hidden="true" />{{ t('utilities.setup.check_again') }}
              </button>
            </template>
            <button v-else type="button" class="btn-primary" :disabled="wb.installing.value" @click="wb.installPdf2docx">
              <Loader2 v-if="wb.installing.value" class="w-4 h-4 animate-spin" aria-hidden="true" /><Download v-else class="w-4 h-4" aria-hidden="true" />
              {{ wb.installing.value ? t('utilities.setup.installing') : t('utilities.setup.install') }}
            </button>
          </div>
          <p v-if="wb.installError.value" role="alert" class="text-sm text-danger mt-3 mb-0">{{ t('utilities.errors.install_failed', { reason: wb.installError.value }) }}</p>
          <p v-if="tool.needs === 'libreoffice'" class="text-xs text-muted dark:text-muted-dark mt-3 mb-0">{{ t('utilities.setup.brew_hint') }}</p>
        </div>

        <!-- Not on this machine -->
        <div v-else-if="avail !== 'ready' && wb.phase.value !== 'done'" class="max-w-[560px] mx-auto mt-10 rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark p-6">
          <h2 class="flex items-center gap-2.5 text-lg font-semibold m-0 text-text dark:text-text-dark">
            <Info class="w-5 h-5" aria-hidden="true" />{{ t('utilities.vision.unavailable_title', { tool: t(`utilities.tools.${tool.id}.name`) }) }}
          </h2>
          <p class="text-text-secondary dark:text-text-secondary-dark leading-relaxed mt-2 mb-0">{{ t(`utilities.vision.${avail === 'not-mac' ? 'not_mac' : 'old_mac'}`) }}</p>
        </div>

        <!-- 1. Empty -->
        <div
          v-else-if="wb.phase.value === 'empty'"
          class="h-full min-h-[380px] rounded-2xl border-2 border-dashed border-border-subtle dark:border-border-subtle-dark bg-surface dark:bg-surface-dark flex flex-col items-center justify-center gap-2.5 text-center p-6"
        >
          <span class="w-16 h-16 rounded-2xl grid place-items-center" :class="`utility-tint-${tool.category}`">
            <component :is="tool.icon" class="w-7 h-7" aria-hidden="true" />
          </span>
          <h2 class="text-lg font-semibold mt-1.5 mb-0 text-text dark:text-text-dark">
            {{ t(tool.max === 1 ? 'utilities.drop.choose_one' : 'utilities.drop.choose', { what: acceptLabel }) }}
          </h2>
          <p class="m-0 text-text-secondary dark:text-text-secondary-dark">
            {{ t('utilities.drop.or_drag') }}<template v-if="tool.min > 1"> · {{ t('utilities.drop.at_least', { n: tool.min }) }}</template>
          </p>
          <div class="flex gap-2 mt-2">
            <button type="button" class="btn-primary" @click="emit('pick')">
              <Upload class="w-4 h-4" aria-hidden="true" />{{ t('utilities.drop.pick') }}
            </button>
          </div>
          <p class="text-xs text-muted dark:text-muted-dark mt-1.5 m-0">{{ t('utilities.drop.untouched') }}</p>
        </div>

        <!-- 4. Done -->
        <ResultPanel
          v-else-if="wb.phase.value === 'done' && wb.job.value"
          :job="wb.job.value"
          :before="wb.before.value"
          @chain="emit('chain', $event)"
          @saved="onSaved"
          @discarded="emit('discarded')"
        />

        <!-- 2 and 3. Files in, maybe running -->
        <template v-else>
          <!-- While it runs, the files are out of reach of the keyboard too, not only the mouse. -->
          <div :inert="wb.phase.value === 'running' || undefined">
            <div class="flex items-center gap-2.5 mb-3.5 text-sm text-text-secondary dark:text-text-secondary-dark">
              <template v-if="tool.workspace === 'files'">
                <span>{{ t('utilities.files.count', { n: wb.usable.value.length }, wb.usable.value.length) }}</span>
                <span v-if="tool.id === 'merge' || tool.id === 'img2pdf'">· {{ t('utilities.files.drag_to_order') }}</span>
                <span class="flex-1" />
                <button v-if="wb.items.value.length > 1" type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] !border-transparent" @click="wb.sortByName()">{{ t('utilities.files.sort_by_name') }}</button>
                <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] !border-transparent" @click="wb.clearItems()">{{ t('utilities.files.clear') }}</button>
              </template>
              <template v-else-if="tool.workspace === 'crop' && firstImage">
                <span class="truncate">{{ firstImage.input.name }}</span>
                <span class="flex-1" />
                <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] !border-transparent" @click="emit('pick')">
                  <FolderOpen class="w-4 h-4" aria-hidden="true" />{{ t('utilities.files.other_file') }}
                </button>
              </template>
              <template v-else-if="wb.firstPdf.value">
                <span class="truncate">{{ wb.firstPdf.value.input.name }} · {{ t('utilities.files.pages', { n: wb.firstPdf.value.pages ?? 0 }, wb.firstPdf.value.pages ?? 0) }}</span>
                <span v-if="tool.id === 'organize'" class="hidden lg:inline text-muted dark:text-muted-dark">· {{ t('utilities.pages.keys') }}</span>
                <span class="flex-1" />
                <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] !border-transparent" @click="emit('pick')">
                  <FolderOpen class="w-4 h-4" aria-hidden="true" />{{ t('utilities.files.other_file') }}
                </button>
              </template>
            </div>

            <p v-if="tooFew" class="mb-3 flex items-center gap-2 text-sm text-text-secondary dark:text-text-secondary-dark">
              <Info class="w-4 h-4" aria-hidden="true" />{{ t('utilities.drop.at_least', { n: tool.min }) }}
            </p>

            <div v-if="tool.workspace === 'preview' && wb.firstPdf.value?.pages" class="flex flex-wrap gap-6 items-start">
              <MarkPreview
                :path="wb.firstPdf.value.input.path"
                :mode="tool.id === 'numbers' ? 'numbers' : 'watermark'"
                :page-count="wb.firstPdf.value.pages"
              />
              <div class="flex-1 min-w-[180px]">
                <p class="text-sm text-text-secondary dark:text-text-secondary-dark mt-0 mb-3">
                  {{ t('utilities.preview.applies_to', { n: wb.usable.value.length }, wb.usable.value.length) }}
                </p>
                <FileCards :items="wb.items.value" :ordered="false" :can-add="canAdd" @remove="wb.removeItem($event)" @add="emit('pick')" />
              </div>
            </div>
            <div v-else-if="tool.workspace === 'cutout'" class="flex flex-wrap gap-6 items-start">
              <CutoutPreview :items="wb.usable.value.filter((i) => i.kind === 'image' && i.width)" class="flex-1 min-w-[320px]" />
              <div class="w-[320px]">
                <FileCards :items="wb.items.value" :ordered="false" :can-add="canAdd" @remove="wb.removeItem($event)" @add="emit('pick')" />
              </div>
            </div>
            <CropBox
              v-else-if="tool.workspace === 'crop' && firstImage?.width"
              :path="firstImage.input.path"
              :width="firstImage.width"
              :height="firstImage.height!"
            />
            <FileCards
              v-else-if="tool.workspace === 'files'"
              :items="wb.items.value"
              :ordered="tool.id === 'merge' || tool.id === 'img2pdf'"
              :can-add="canAdd"
              @remove="wb.removeItem($event)"
              @move="(from, to) => wb.moveItem(from, to)"
              @add="emit('pick')"
            />
            <template v-else-if="wb.firstPdf.value?.pages">
              <PageGrid
                v-if="tool.id === 'split'"
                mode="split"
                :path="wb.firstPdf.value.input.path"
                :page-count="wb.firstPdf.value.pages"
                :parts="wb.splitPlan.value.parts"
                :picking="wb.options.split.mode === 'pick'"
                :picked="wb.options.split.picked"
                @toggle="togglePage"
              />
              <PageGrid
                v-else
                mode="organize"
                :path="wb.firstPdf.value.input.path"
                :page-count="wb.firstPdf.value.pages"
                :slots="wb.options.organize.pages"
                @turn="turnPage"
                @drop-page="leaveOut"
                @move="movePage"
              />
            </template>
            <FileCards
              v-else
              :items="wb.items.value"
              :ordered="false"
              :can-add="false"
              @remove="wb.removeItem($event)"
            />
          </div>
        </template>
      </div>
      <!-- 3. Running: over the scrolling area, not in it, so it stays in view however far down the pages were scrolled -->
      <div v-if="wb.phase.value === 'running'" class="absolute inset-0 grid place-items-center bg-base/80 dark:bg-base-dark/80 backdrop-blur-[2px]">
        <div role="status" class="w-[360px] rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark p-5 shadow-xl">
          <p class="m-0 font-semibold text-text dark:text-text-dark">{{ t(`utilities.tools.${tool.id}.running`) }}</p>
          <div class="h-1.5 rounded-full bg-surface-hover dark:bg-surface-hover-dark overflow-hidden mt-3 mb-2">
            <div class="h-full rounded-full bg-accent transition-[width] duration-200" :style="{ width: `${percent}%` }" />
          </div>
          <div class="flex items-center justify-between text-[13px] text-text-secondary dark:text-text-secondary-dark">
            <span class="tabular-nums">{{ wb.progress.total ? t('utilities.progress.of', { done: wb.progress.done, total: wb.progress.total }) : t('utilities.progress.starting') }}</span>
            <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] !border-transparent" @click="wb.cancel()">{{ t('utilities.progress.cancel') }}</button>
          </div>
        </div>
      </div>
    </div>

    <!-- The options -->
    <aside
      v-if="avail === 'ready' && (wb.phase.value === 'loaded' || wb.phase.value === 'running')"
      class="w-[316px] shrink-0 border-l border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark overflow-auto flex flex-col"
      :class="wb.phase.value === 'running' ? 'pointer-events-none opacity-60' : ''"
      :inert="wb.phase.value === 'running' || undefined"
      :aria-label="t('utilities.options.title')"
    >
      <section class="px-[18px] py-4 border-b border-border dark:border-border-dark">
        <h3 class="utility-label">{{ t('utilities.options.title') }}</h3>

        <!-- Merge -->
        <p v-if="tool.id === 'merge'" class="text-[13px] text-text-secondary dark:text-text-secondary-dark m-0">{{ t('utilities.options.merge_hint') }}</p>

        <!-- Split -->
        <template v-else-if="tool.id === 'split'">
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.split_by')">
            <button v-for="m in (['ranges', 'every', 'pick'] as const)" :key="m" type="button" role="radio" :aria-checked="wb.options.split.mode === m" :class="{ on: wb.options.split.mode === m }" @click="wb.options.split.mode = m">
              {{ t(`utilities.options.split_${m}`) }}
            </button>
          </div>
          <div v-if="wb.options.split.mode === 'ranges'" class="mt-3">
            <input v-model="wb.options.split.ranges" class="utility-field" :aria-invalid="!!wb.splitPlan.value.bad" :aria-label="t('utilities.options.ranges')" :placeholder="t('utilities.options.ranges_placeholder')" />
            <p v-if="wb.splitPlan.value.bad" class="text-xs text-danger mt-1.5 mb-0">{{ t('utilities.options.ranges_bad', { part: wb.splitPlan.value.bad, n: wb.firstPdf.value?.pages ?? 0 }) }}</p>
            <p v-else class="text-xs text-muted dark:text-muted-dark mt-1.5 mb-0">{{ t('utilities.options.ranges_hint') }}</p>
          </div>
          <div v-else-if="wb.options.split.mode === 'every'" class="mt-3 flex items-center gap-2 text-sm text-text-secondary dark:text-text-secondary-dark">
            <label for="utility-every">{{ t('utilities.options.every') }}</label>
            <input id="utility-every" v-model.number="wb.options.split.every" type="number" min="1" :max="wb.firstPdf.value?.pages ?? 1" class="utility-field !w-20" />
            <span>{{ t('utilities.options.pages_word') }}</span>
          </div>
          <p v-else class="text-xs text-muted dark:text-muted-dark mt-2.5 mb-0">{{ t('utilities.options.pick_hint') }}</p>
        </template>

        <!-- Organize -->
        <template v-else-if="tool.id === 'organize'">
          <p class="text-[13px] text-text-secondary dark:text-text-secondary-dark m-0">{{ t('utilities.options.organize_hint') }}</p>
          <p class="text-[13px] text-text-secondary dark:text-text-secondary-dark mt-2 mb-0">
            {{ t('utilities.options.organize_count', { kept: wb.options.organize.pages.length, total: wb.firstPdf.value?.pages ?? 0 }) }}
          </p>
          <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] mt-3" @click="resetOrganize">{{ t('utilities.options.organize_reset') }}</button>
        </template>

        <!-- Rotate -->
        <div v-else-if="tool.id === 'rotate'" class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.rotate_by')">
          <button v-for="a in ([90, 180, 270] as const)" :key="a" type="button" role="radio" :aria-checked="wb.options.rotate.angle === a" :class="{ on: wb.options.rotate.angle === a }" @click="wb.options.rotate.angle = a">
            {{ t(`utilities.options.rotate_${a}`) }}
          </button>
        </div>

        <!-- Images to PDF -->
        <template v-else-if="tool.id === 'img2pdf'">
          <p class="utility-sub">{{ t('utilities.options.page_size') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.page_size')">
            <button v-for="s in (['a4', 'letter', 'fit'] as const)" :key="s" type="button" role="radio" :aria-checked="wb.options.img2pdf.size === s" :class="{ on: wb.options.img2pdf.size === s }" @click="wb.options.img2pdf.size = s">
              {{ t(`utilities.options.size_${s}`) }}
            </button>
          </div>
          <template v-if="wb.options.img2pdf.size !== 'fit'">
            <p class="utility-sub">{{ t('utilities.options.orientation') }}</p>
            <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.orientation')">
              <button v-for="o in (['auto', 'portrait', 'landscape'] as const)" :key="o" type="button" role="radio" :aria-checked="wb.options.img2pdf.orientation === o" :class="{ on: wb.options.img2pdf.orientation === o }" @click="wb.options.img2pdf.orientation = o">
                {{ t(`utilities.options.orientation_${o}`) }}
              </button>
            </div>
            <p class="utility-sub">{{ t('utilities.options.margin') }}</p>
            <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.margin')">
              <button v-for="[key, pt] in ([['none', 0], ['small', 18], ['medium', 36]] as const)" :key="key" type="button" role="radio" :aria-checked="wb.options.img2pdf.margin === pt" :class="{ on: wb.options.img2pdf.margin === pt }" @click="wb.options.img2pdf.margin = pt">
                {{ t(`utilities.options.margin_${key}`) }}
              </button>
            </div>
          </template>
        </template>

        <!-- Watermark -->
        <template v-else-if="tool.id === 'watermark'">
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.mark_kind')">
            <button v-for="m in (['text', 'image'] as const)" :key="m" type="button" role="radio" :aria-checked="wb.options.watermark.mode === m" :class="{ on: wb.options.watermark.mode === m }" @click="wb.options.watermark.mode = m">
              {{ t(`utilities.options.mark_${m}`) }}
            </button>
          </div>
          <template v-if="wb.options.watermark.mode === 'text'">
            <input v-model="wb.options.watermark.text" class="utility-field mt-2.5" :aria-label="t('utilities.options.mark_text')" :placeholder="t('utilities.options.mark_text_placeholder')" />
            <div class="flex gap-1.5 mt-2.5" role="radiogroup" :aria-label="t('utilities.options.mark_color')">
              <button
                v-for="c in ['#d93838', '#1c1c1e', '#71717a', '#1a66cc', '#16a34a']"
                :key="c"
                type="button"
                role="radio"
                :aria-checked="wb.options.watermark.color === c"
                :aria-label="c"
                class="w-7 h-7 rounded-md border border-border dark:border-border-dark"
                :class="wb.options.watermark.color === c ? 'ring-2 ring-accent ring-offset-2 ring-offset-surface-alt dark:ring-offset-surface-alt-dark' : ''"
                :style="{ background: c }"
                @click="wb.options.watermark.color = c"
              />
            </div>
          </template>
          <div v-else class="mt-2.5 flex items-center gap-2">
            <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px]" @click="pickLogo">
              <Upload class="w-4 h-4" aria-hidden="true" />{{ wb.options.watermark.logo ? t('utilities.options.mark_other_logo') : t('utilities.options.mark_pick_logo') }}
            </button>
            <span v-if="wb.options.watermark.logo" class="text-xs text-muted truncate">{{ wb.options.watermark.logo.name }}</span>
          </div>

          <p class="utility-sub">{{ t('utilities.options.position') }}</p>
          <div class="flex gap-4 items-start">
            <div class="grid grid-cols-3 gap-1" role="radiogroup" :aria-label="t('utilities.options.position')">
              <button
                v-for="i in 9"
                :key="i"
                type="button"
                role="radio"
                :aria-checked="!wb.options.watermark.tile && wb.options.watermark.position === i - 1"
                :aria-label="t(`utilities.options.place_${i - 1}`)"
                :disabled="wb.options.watermark.tile"
                class="w-7 h-7 rounded-md border border-border dark:border-border-dark disabled:opacity-40"
                :class="!wb.options.watermark.tile && wb.options.watermark.position === i - 1 ? 'bg-accent border-accent' : 'bg-surface dark:bg-surface-dark'"
                @click="wb.options.watermark.position = i - 1"
              />
            </div>
            <div class="flex flex-col gap-2 text-[13px] text-text-secondary dark:text-text-secondary-dark">
              <label class="flex items-center gap-2"><input v-model="wb.options.watermark.tile" type="checkbox" />{{ t('utilities.options.mark_tile') }}</label>
              <label class="flex items-center gap-2"><input v-model="wb.options.watermark.under" type="checkbox" />{{ t('utilities.options.mark_under') }}</label>
              <label class="flex items-center gap-2"><input v-model="wb.options.watermark.exceptFirst" type="checkbox" />{{ t('utilities.options.except_first') }}</label>
            </div>
          </div>

          <label class="utility-sub block" for="utility-mark-size">{{ t('utilities.options.mark_size') }}</label>
          <div class="flex items-center gap-2.5">
            <input id="utility-mark-size" v-model.number="wb.options.watermark.size" type="range" min="0.1" max="1" step="0.05" class="flex-1 accent-accent" />
            <span class="w-11 text-right text-[13px] tabular-nums">{{ Math.round(wb.options.watermark.size * 100) }}%</span>
          </div>
          <label class="utility-sub block" for="utility-mark-opacity">{{ t('utilities.options.mark_opacity') }}</label>
          <div class="flex items-center gap-2.5">
            <input id="utility-mark-opacity" v-model.number="wb.options.watermark.opacity" type="range" min="0.05" max="1" step="0.05" class="flex-1 accent-accent" />
            <span class="w-11 text-right text-[13px] tabular-nums">{{ Math.round(wb.options.watermark.opacity * 100) }}%</span>
          </div>
          <label class="utility-sub block" for="utility-mark-rotate">{{ t('utilities.options.mark_rotate') }}</label>
          <div class="flex items-center gap-2.5">
            <input id="utility-mark-rotate" v-model.number="wb.options.watermark.rotate" type="range" min="-90" max="90" step="5" class="flex-1 accent-accent" />
            <span class="w-11 text-right text-[13px] tabular-nums">{{ wb.options.watermark.rotate }}°</span>
          </div>
        </template>

        <!-- Page numbers -->
        <template v-else-if="tool.id === 'numbers'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.position') }}</p>
          <div class="grid grid-cols-3 gap-1 w-fit" role="radiogroup" :aria-label="t('utilities.options.position')">
            <button
              v-for="i in 6"
              :key="i"
              type="button"
              role="radio"
              :aria-checked="wb.options.numbers.position === i - 1"
              :aria-label="t(`utilities.options.number_place_${i - 1}`)"
              class="w-9 h-6 rounded-md border border-border dark:border-border-dark"
              :class="[wb.options.numbers.position === i - 1 ? 'bg-accent border-accent' : 'bg-surface dark:bg-surface-dark', i === 4 ? 'mt-3' : '', i > 3 ? 'mt-3' : '']"
              @click="wb.options.numbers.position = i - 1"
            />
          </div>
          <label class="utility-sub block" for="utility-number-format">{{ t('utilities.options.number_format') }}</label>
          <select id="utility-number-format" v-model="wb.options.numbers.format" class="utility-field">
            <option v-for="f in NUMBER_FORMATS" :key="f" :value="f">{{ numberText(f, 3, 12) }}</option>
          </select>
          <div class="flex gap-3 mt-3">
            <label class="flex-1 text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('utilities.options.number_start') }}
              <input v-model.number="wb.options.numbers.start" type="number" min="0" class="utility-field mt-1" />
            </label>
            <label class="flex-1 text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('utilities.options.number_size') }}
              <input v-model.number="wb.options.numbers.size" type="number" min="6" max="36" class="utility-field mt-1" />
            </label>
          </div>
          <label class="flex items-center gap-2 mt-3 text-[13px] text-text-secondary dark:text-text-secondary-dark">
            <input v-model="wb.options.numbers.skipFirst" type="checkbox" />{{ t('utilities.options.skip_cover') }}
          </label>
        </template>

        <!-- Protect -->
        <template v-else-if="tool.id === 'protect'">
          <label class="text-xs text-text-tertiary dark:text-text-tertiary-dark block">{{ t('utilities.options.password') }}
            <span class="relative block mt-1">
              <input v-model="wb.options.protect.password" :type="showPassword ? 'text' : 'password'" autocomplete="new-password" class="utility-field pr-9" />
              <button type="button" class="absolute right-1 top-1 w-7 h-7 grid place-items-center text-muted" :aria-label="t(showPassword ? 'utilities.options.hide_password' : 'utilities.options.show_password')" @click="showPassword = !showPassword">
                <EyeOff v-if="showPassword" class="w-4 h-4" aria-hidden="true" /><Eye v-else class="w-4 h-4" aria-hidden="true" />
              </button>
            </span>
          </label>
          <label class="text-xs text-text-tertiary dark:text-text-tertiary-dark block mt-3">{{ t('utilities.options.password_again') }}
            <input v-model="wb.options.protect.confirm" :type="showPassword ? 'text' : 'password'" autocomplete="new-password" class="utility-field mt-1" :aria-invalid="mismatch" />
          </label>
          <p v-if="mismatch" class="text-xs text-danger mt-1.5 mb-0">{{ t('utilities.options.password_mismatch') }}</p>
          <p v-if="previewMayRefuse(wb.options.protect.password)" role="status" class="text-xs mt-2 mb-0 text-warning flex gap-1.5">
            <TriangleAlert class="w-3.5 h-3.5 shrink-0 mt-px" aria-hidden="true" />{{ t('utilities.options.password_ascii') }}
          </p>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.protect_hint') }}</p>
        </template>

        <!-- Unlock -->
        <template v-else-if="tool.id === 'unlock'">
          <label class="text-xs text-text-tertiary dark:text-text-tertiary-dark block">{{ t('utilities.options.password') }}
            <input v-model="wb.options.unlock.password" type="password" autocomplete="off" class="utility-field mt-1" />
          </label>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.unlock_hint') }}</p>
        </template>

        <!-- PDF to text -->
        <p v-else-if="tool.id === 'pdf2text'" class="text-[13px] text-text-secondary dark:text-text-secondary-dark m-0">{{ t('utilities.options.text_hint') }}</p>

        <!-- Compress images -->
        <template v-else-if="tool.id === 'imgcompress'">
          <label class="utility-sub !mt-0 block" for="utility-quality">{{ t('utilities.options.image_quality') }}</label>
          <div class="flex items-center gap-2.5">
            <input id="utility-quality" v-model.number="wb.options.imgcompress.quality" type="range" min="0.3" max="0.95" step="0.05" class="flex-1 accent-accent" />
            <span class="w-11 text-right text-[13px] tabular-nums">{{ Math.round(wb.options.imgcompress.quality * 100) }}</span>
          </div>
          <p class="utility-sub">{{ t('utilities.options.format') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.format')">
            <button v-for="f in (['keep', 'jpeg', ...(wb.webp.value ? ['webp'] : [])] as const)" :key="f" type="button" role="radio" :aria-checked="wb.options.imgcompress.format === f" :class="{ on: wb.options.imgcompress.format === f }" @click="wb.options.imgcompress.format = f as typeof wb.options.imgcompress.format">
              {{ f === 'keep' ? t('utilities.options.format_keep') : f === 'jpeg' ? 'JPG' : 'WebP' }}
            </button>
          </div>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.compress_hint') }}</p>
        </template>

        <!-- Resize -->
        <template v-else-if="tool.id === 'resize'">
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.resize_by')">
            <button v-for="m in (['percent', 'fit'] as const)" :key="m" type="button" role="radio" :aria-checked="wb.options.resize.mode === m" :class="{ on: wb.options.resize.mode === m }" @click="wb.options.resize.mode = m">
              {{ t(`utilities.options.resize_${m}`) }}
            </button>
          </div>
          <div v-if="wb.options.resize.mode === 'percent'" class="flex items-center gap-2.5 mt-3">
            <input v-model.number="wb.options.resize.percent" type="range" min="5" max="200" step="5" class="flex-1 accent-accent" :aria-label="t('utilities.options.resize_percent')" />
            <span class="w-12 text-right text-[13px] tabular-nums">{{ wb.options.resize.percent }}%</span>
          </div>
          <template v-else>
            <div class="flex items-center gap-2 mt-3 text-sm">
              <input v-model.number="wb.options.resize.width" type="number" min="1" class="utility-field" :aria-label="t('utilities.options.max_width')" />
              <span aria-hidden="true">×</span>
              <input v-model.number="wb.options.resize.height" type="number" min="1" class="utility-field" :aria-label="t('utilities.options.max_height')" />
            </div>
            <p class="text-xs text-muted dark:text-muted-dark mt-1.5 mb-0">{{ t('utilities.options.fit_hint') }}</p>
            <label class="flex items-center gap-2 mt-2.5 text-[13px] text-text-secondary dark:text-text-secondary-dark">
              <input v-model="wb.options.resize.enlarge" type="checkbox" />{{ t('utilities.options.enlarge') }}
            </label>
          </template>
          <p v-if="firstImage?.width" class="text-xs text-muted dark:text-muted-dark mt-2.5 mb-0 tabular-nums">
            {{ t('utilities.options.now_size', { w: firstImage.width, h: firstImage.height }) }}
          </p>
        </template>

        <!-- Crop -->
        <template v-else-if="tool.id === 'crop'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.aspect') }}</p>
          <div class="grid grid-cols-3 gap-1.5" role="radiogroup" :aria-label="t('utilities.options.aspect')">
            <button
              v-for="a in ['free', '1:1', '4:3', '3:4', '16:9', '9:16']"
              :key="a"
              type="button"
              role="radio"
              :aria-checked="wb.options.crop.aspect === a"
              class="h-8 rounded-md border text-[13px]"
              :class="wb.options.crop.aspect === a ? 'border-accent text-accent dark:text-accent-dark bg-accent/10' : 'border-border dark:border-border-dark bg-surface dark:bg-surface-dark'"
              @click="setAspect(a)"
            >{{ a === 'free' ? t('utilities.options.aspect_free') : a }}</button>
          </div>
          <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px] mt-3" @click="wb.resetCrop()">{{ t('utilities.options.crop_reset') }}</button>
        </template>

        <!-- Convert images -->
        <div v-else-if="tool.id === 'imgconvert'" class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.format')">
          <button v-for="f in formats" :key="f" type="button" role="radio" :aria-checked="wb.options.imgconvert.format === f" :class="{ on: wb.options.imgconvert.format === f }" @click="wb.options.imgconvert.format = f">
            {{ f === 'jpeg' ? 'JPG' : f.toUpperCase() }}
          </button>
        </div>

        <!-- Compress PDF -->
        <template v-else-if="tool.id === 'compress'">
          <div class="flex flex-col gap-2" role="radiogroup" :aria-label="t('utilities.options.compress_level')">
            <button
              v-for="(level, i) in COMPRESS_LEVELS"
              :key="i"
              type="button"
              role="radio"
              :aria-checked="wb.options.compress.level === i"
              class="text-left rounded-[9px] border px-3 py-2.5 bg-surface dark:bg-surface-dark"
              :class="wb.options.compress.level === i ? 'border-accent ring-1 ring-accent' : 'border-border dark:border-border-dark'"
              @click="wb.options.compress.level = i as 0 | 1 | 2"
            >
              <span class="block font-semibold text-sm text-text dark:text-text-dark">{{ t(`utilities.options.compress_${i}`) }}</span>
              <span class="block text-[12.5px] text-text-secondary dark:text-text-secondary-dark mt-0.5">{{ t(`utilities.options.compress_${i}_hint`, { dpi: level.dpi }) }}</span>
            </button>
          </div>
          <p class="text-xs text-muted dark:text-muted-dark mt-2.5 mb-0">{{ t('utilities.options.compress_pdf_hint') }}</p>
        </template>

        <!-- Office to PDF, PDF to Word -->
        <p v-else-if="tool.id === 'office2pdf' || tool.id === 'pdf2word'" class="text-[13px] text-text-secondary dark:text-text-secondary-dark m-0">{{ t(`utilities.options.${tool.id}_hint`) }}</p>

        <!-- Remove background -->
        <template v-else-if="tool.id === 'removebg'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.backdrop') }}</p>
          <div class="flex gap-1.5 flex-wrap items-center" role="radiogroup" :aria-label="t('utilities.options.backdrop')">
            <button
              v-for="c in ['transparent', '#ffffff', '#111111', '#dbeafe', '#dcfce7']"
              :key="c"
              type="button"
              role="radio"
              :aria-checked="wb.options.removebg.backdrop === c"
              :aria-label="c === 'transparent' ? t('utilities.options.backdrop_none') : c"
              :title="c === 'transparent' ? t('utilities.options.backdrop_none') : c"
              class="w-7 h-7 rounded-md border border-border dark:border-border-dark"
              :class="wb.options.removebg.backdrop === c ? 'ring-2 ring-accent ring-offset-2 ring-offset-surface-alt dark:ring-offset-surface-alt-dark' : ''"
              :style="c === 'transparent'
                ? 'background-color:#fff;background-image:linear-gradient(45deg,#ddd 25%,transparent 25%),linear-gradient(-45deg,#ddd 25%,transparent 25%),linear-gradient(45deg,transparent 75%,#ddd 75%),linear-gradient(-45deg,transparent 75%,#ddd 75%);background-size:10px 10px;background-position:0 0,0 5px,5px -5px,-5px 0'
                : { background: c }"
              @click="wb.options.removebg.backdrop = c"
            />
            <label class="w-7 h-7 rounded-md border border-border dark:border-border-dark overflow-hidden cursor-pointer" :title="t('utilities.options.backdrop_other')">
              <input type="color" class="w-10 h-10 -m-1.5 cursor-pointer" :aria-label="t('utilities.options.backdrop_other')" @input="wb.options.removebg.backdrop = ($event.target as HTMLInputElement).value" />
            </label>
          </div>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t(wb.options.removebg.backdrop === 'transparent' ? 'utilities.options.backdrop_png' : 'utilities.options.backdrop_jpg') }}</p>
          <label class="flex items-center gap-2 mt-3 text-[13px] text-text-secondary dark:text-text-secondary-dark">
            <input v-model="wb.options.removebg.trim" type="checkbox" />{{ t('utilities.options.trim') }}
          </label>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.removebg_hint') }}</p>
        </template>

        <!-- OCR -->
        <template v-else-if="tool.id === 'ocr'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.ocr_makes') }}</p>
          <label class="flex items-center gap-2 text-[13px] text-text-secondary dark:text-text-secondary-dark"><input v-model="wb.options.ocr.pdf" type="checkbox" />{{ t('utilities.options.ocr_pdf') }}</label>
          <label class="flex items-center gap-2 mt-1.5 text-[13px] text-text-secondary dark:text-text-secondary-dark"><input v-model="wb.options.ocr.text" type="checkbox" />{{ t('utilities.options.ocr_text') }}</label>
          <label class="flex items-center gap-2 mt-3 text-[13px] text-text-secondary dark:text-text-secondary-dark"><input v-model="wb.options.ocr.skipText" type="checkbox" />{{ t('utilities.options.skip_text_pages') }}</label>
          <p class="utility-sub">{{ t('utilities.options.language') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.language')">
            <button v-for="l in (['vi', 'auto'] as const)" :key="l" type="button" role="radio" :aria-checked="wb.options.ocr.language === l" :class="{ on: wb.options.ocr.language === l }" @click="wb.options.ocr.language = l">
              {{ t(`utilities.options.language_${l}`) }}
            </button>
          </div>
          <p v-if="!hasVietnamese" class="text-xs text-warning mt-2 mb-0">{{ t('utilities.options.no_vietnamese') }}</p>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.ocr_hint') }}</p>
        </template>

        <!-- Text from images -->
        <template v-else-if="tool.id === 'imgtext'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.language') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.language')">
            <button v-for="l in (['vi', 'auto'] as const)" :key="l" type="button" role="radio" :aria-checked="wb.options.imgtext.language === l" :class="{ on: wb.options.imgtext.language === l }" @click="wb.options.imgtext.language = l">
              {{ t(`utilities.options.language_${l}`) }}
            </button>
          </div>
          <p v-if="!hasVietnamese" class="text-xs text-warning mt-2 mb-0">{{ t('utilities.options.no_vietnamese') }}</p>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.imgtext_hint') }}</p>
        </template>

        <!-- Remove metadata -->
        <p v-else-if="tool.id === 'exif'" class="text-[13px] text-text-secondary dark:text-text-secondary-dark m-0">{{ t('utilities.options.exif_hint') }}</p>

        <!-- PDF to images -->
        <template v-else-if="tool.id === 'pdf2img'">
          <p class="utility-sub !mt-0">{{ t('utilities.options.format') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.format')">
            <button v-for="f in (['png', 'jpeg'] as const)" :key="f" type="button" role="radio" :aria-checked="wb.options.pdf2img.format === f" :class="{ on: wb.options.pdf2img.format === f }" @click="wb.options.pdf2img.format = f">
              {{ f === 'png' ? 'PNG' : 'JPG' }}
            </button>
          </div>
          <p class="utility-sub">{{ t('utilities.options.quality') }}</p>
          <div class="utility-seg" role="radiogroup" :aria-label="t('utilities.options.quality')">
            <button v-for="d in ([72, 150, 300] as const)" :key="d" type="button" role="radio" :aria-checked="wb.options.pdf2img.dpi === d" :class="{ on: wb.options.pdf2img.dpi === d }" @click="wb.options.pdf2img.dpi = d">
              {{ t(`utilities.options.dpi_${d}`) }}
            </button>
          </div>
          <p class="text-xs text-muted dark:text-muted-dark mt-2 mb-0">{{ t('utilities.options.png_hint') }}</p>
        </template>
      </section>

      <section class="px-[18px] py-4 mt-auto border-t border-border dark:border-border-dark">
        <h3 class="utility-label">{{ t('utilities.options.output') }}</h3>
        <p class="text-sm font-medium text-text dark:text-text-dark m-0 break-all">{{ plan }}</p>
        <p v-if="tool.id === 'split' && splitSummary" class="text-xs text-muted dark:text-muted-dark mt-1 mb-0">{{ splitSummary }}</p>
        <p class="text-xs text-muted dark:text-muted-dark mt-1.5 mb-0">{{ t('utilities.options.output_hint') }}</p>
      </section>
    </aside>
  </div>
</template>
