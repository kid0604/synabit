<script setup lang="ts">
/**
 * What a run made, and the ways to keep it (design §5.3, state 4, and §5.4).
 *
 * Nothing has been written to the person's files yet: the result waits in the
 * workspace, and this says for how long. One file gets a card with its name
 * editable in place; several get a list, saved all at once into a folder or a
 * .zip. "Use with" hands the result to the next tool without a save between.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Check, Clock, Copy, Download, ExternalLink, FileText, FolderInput, FolderOpen, Save, Trash2 } from 'lucide-vue-next';
import PdfThumb from './PdfThumb.vue';
import * as api from '../api';
import { closePdf, fileUrl } from '../render';
import { availability, formatSize, toolById, type ToolId } from '../tools';
import { useWorkbench } from '../useWorkbench';
import { useAppStore } from '../../../stores/useAppStore';
import { showAppNotice } from '../../../composables/useAppNotice';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';
import { KEEP_HOURS } from '../constants';

const props = defineProps<{ job: api.Job; /** Bytes in, for a tool that makes things smaller. */ before?: number }>();
const emit = defineEmits<{
  (e: 'chain', tool: ToolId): void;
  (e: 'saved', job: api.Job): void;
  (e: 'discarded'): void;
}>();

const { t, locale } = useI18n();
const appStore = useAppStore();

const outputs = computed(() => props.job.outputs);
const single = computed(() => outputs.value.length === 1);
const first = computed(() => outputs.value[0]);
const isPdf = (name: string) => name.toLowerCase().endsWith('.pdf');
const isImage = (name: string) => /\.(jpe?g|png|webp|gif)$/i.test(name);
const smaller = computed(() => {
  if (!props.before || !['imgcompress', 'compress'].includes(props.job.tool)) return null;
  const change = Math.round((1 - total.value / props.before) * 100);
  return { from: formatSize(props.before, locale.value), change };
});
/** The folder in the vault the original is in, for the button's words. */
const besideFolder = computed(() => {
  const dir = props.job.originDir;
  if (!dir) return null;
  const vault = appStore.vaultPath;
  const inside = vault && dir.startsWith(vault) ? dir.slice(vault.length).replace(/^[\\/]+/, '') : '';
  return inside || t('utilities.result.vault_root');
});
const total = computed(() => outputs.value.reduce((n, o) => n + o.size, 0));

// The results' previews open them in pdf.js; let them go with the panel.
const letGo = (job: api.Job) => job.outputs.filter((o) => isPdf(o.name)).forEach((o) => void closePdf(o.path));
watch(() => props.job, (now, old) => old && old.id !== now.id && letGo(old));
onBeforeUnmount(() => letGo(props.job));

const name = ref(first.value?.name ?? '');
watch(first, (o) => (name.value = o?.name ?? ''));

const hoursLeft = computed(() => {
  const left = props.job.createdMs + KEEP_HOURS * 3600_000 - Date.now();
  return Math.max(1, Math.ceil(left / 3600_000));
});

/** What to offer next, for a PDF result and for pictures. */
const wb = useWorkbench();
const next = computed<ToolId[]>(() => offered.value.filter((id) => availability(toolById(id)!, wb.vision.value, wb.office.value) === 'ready'));
const offered = computed<ToolId[]>(() => {
  if (!outputs.value.length) return [];
  const tool = props.job.tool as ToolId;
  if (outputs.value.every((o) => isPdf(o.name))) {
    const all: ToolId[] = outputs.value.length > 1
      ? ['merge', 'compress', 'watermark', 'numbers', 'protect', 'pdf2img']
      : ['compress', 'split', 'watermark', 'protect', 'pdf2word', 'ocr', 'pdf2img'];
    return all.filter((id) => id !== tool);
  }
  if (outputs.value.every((o) => isImage(o.name))) {
    return (['removebg', 'img2pdf', 'imgcompress', 'resize', 'exif', 'imgtext'] as ToolId[]).filter((id) => id !== tool);
  }
  return [];
});

const busy = ref(false);

function announce(job: api.Job, where: string) {
  emit('saved', job);
  showAppNotice(t('utilities.result.saved_to', { where }), 'info', [
    { label: t('utilities.result.show_in_finder'), run: () => void api.revealSaved(job.id).catch(logger.error) },
  ]);
}

async function guard(work: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  try {
    await work();
  } catch (e) {
    logger.error('[utilities] save failed', e);
    showAppNotice(t('utilities.result.save_failed', { reason: errorText(e) }), 'error');
  } finally {
    busy.value = false;
  }
}

const saveOne = () =>
  guard(async () => {
    const saved = await api.saveAs(props.job.id, first.value.name, name.value || first.value.name);
    if (saved) announce(saved, saved.savedTo ?? '');
  });

const saveBeside = () =>
  guard(async () => {
    const vault = appStore.vaultPath;
    if (!vault) {
      showAppNotice(t('utilities.result.no_vault'), 'error');
      return;
    }
    const where = await api.saveBeside(vault, props.job.id, first.value.name, name.value || first.value.name);
    emit('saved', await api.getJob(props.job.id));
    showAppNotice(t('utilities.result.saved_in_vault', { where }));
  });

const saveToVault = () =>
  guard(async () => {
    const vault = appStore.vaultPath;
    if (!vault) {
      showAppNotice(t('utilities.result.no_vault'), 'error');
      return;
    }
    const where = await api.saveToVault(vault, props.job.id, first.value.name, name.value || first.value.name);
    emit('saved', await api.getJob(props.job.id));
    showAppNotice(t('utilities.result.saved_in_vault', { where }));
  });

const saveAll = () =>
  guard(async () => {
    const saved = await api.saveAllToFolder(props.job.id, t('utilities.result.pick_folder'));
    if (saved) announce(saved, saved.savedTo ?? '');
  });

const saveZip = () =>
  guard(async () => {
    const base = (first.value?.name ?? 'ket-qua').replace(/(-\d+)?\.[^.]+$/, '');
    const saved = await api.saveZip(props.job.id, `${base}.zip`);
    if (saved) announce(saved, saved.savedTo ?? '');
  });

const open = (output: string) => guard(() => api.openOutput(props.job.id, output));

const throwAway = () =>
  guard(async () => {
    await api.discard(props.job.id);
    emit('discarded');
  });
</script>

<template>
  <div class="max-w-[820px] mx-auto">
    <div class="flex items-center gap-2.5 mb-4">
      <span class="w-[30px] h-[30px] rounded-full grid place-items-center bg-success/15 text-success">
        <Check class="w-4 h-4" aria-hidden="true" />
      </span>
      <h2 class="text-lg font-semibold text-text dark:text-text-dark m-0">{{ t('utilities.result.done') }}</h2>
      <span class="text-sm text-muted dark:text-muted-dark">
        {{ single ? formatSize(first.size, locale) : t('utilities.result.files', { n: outputs.length, size: formatSize(total, locale) }, outputs.length) }}
        <template v-if="smaller && !single"> · {{ t('utilities.result.smaller_by', { n: smaller.change }) }}</template>
      </span>
    </div>

    <!-- One file: a card -->
    <div v-if="single && first" class="flex gap-5 rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark p-4">
      <PdfThumb v-if="isPdf(first.name)" :path="first.path" :width="150" />
      <img v-else-if="isImage(first.name)" :src="fileUrl(first.path)" alt="" class="w-[180px] max-h-[220px] object-contain rounded-md border border-border dark:border-border-dark" />
      <span v-else class="w-[150px] h-[190px] grid place-items-center rounded-md border border-border dark:border-border-dark bg-surface-hover dark:bg-surface-hover-dark text-muted">
        <FileText class="w-10 h-10" aria-hidden="true" />
      </span>
      <div class="flex-1 min-w-0 flex flex-col gap-1.5">
        <label class="sr-only" for="utility-result-name">{{ t('utilities.result.name') }}</label>
        <input
          id="utility-result-name"
          v-model="name"
          class="w-full -ml-1.5 px-1.5 py-1 rounded-md border border-transparent hover:border-border dark:hover:border-border-dark focus:border-accent outline-none bg-transparent font-semibold text-[15px] text-text dark:text-text-dark"
          :title="t('utilities.result.rename_hint')"
        />
        <p class="text-sm text-muted dark:text-muted-dark m-0">
          <template v-if="smaller">{{ smaller.from }} → {{ formatSize(first.size, locale) }} <span class="font-semibold" :class="smaller.change > 0 ? 'text-success' : ''">({{ smaller.change > 0 ? '−' : '+' }}{{ Math.abs(smaller.change) }}%)</span></template>
          <template v-else>{{ formatSize(first.size, locale) }}</template>
        </p>
        <p v-if="!job.saved" class="flex items-center gap-1.5 text-xs text-muted dark:text-muted-dark m-0">
          <Clock class="w-3.5 h-3.5" aria-hidden="true" />{{ t('utilities.result.not_saved', { hours: hoursLeft }, hoursLeft) }}
        </p>
        <p v-else class="flex items-center gap-1.5 text-xs text-success m-0">
          <Check class="w-3.5 h-3.5" aria-hidden="true" />{{ t('utilities.result.was_saved') }}
        </p>
        <div class="flex flex-wrap gap-2 mt-auto pt-3">
          <template v-if="besideFolder">
            <button type="button" class="btn-primary" :disabled="busy" :title="t('utilities.result.beside_in', { folder: besideFolder })" @click="saveBeside">
              <Save class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.save_beside') }}
            </button>
            <button type="button" class="btn-secondary" :disabled="busy" @click="saveOne">
              <Download class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.save_elsewhere') }}
            </button>
          </template>
          <template v-else>
            <button type="button" class="btn-primary" :disabled="busy" @click="saveOne">
              <Download class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.save') }}
            </button>
            <button type="button" class="btn-secondary" :disabled="busy" @click="saveToVault">
              <FolderInput class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.add_to_vault') }}
            </button>
          </template>
          <button type="button" class="btn-secondary !border-transparent" :disabled="busy" @click="open(first.name)">
            <ExternalLink class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.open') }}
          </button>
          <button type="button" class="btn-secondary !border-transparent text-muted dark:text-muted-dark" :disabled="busy" @click="throwAway">
            <Trash2 class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.discard') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Several: a list -->
    <div v-else class="flex flex-col gap-2">
      <ul class="flex flex-col gap-2 list-none p-0 m-0 max-h-[50vh] overflow-auto">
        <li v-for="o in outputs" :key="o.name" class="flex items-center gap-3 rounded-xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark px-3 py-2">
          <PdfThumb v-if="isPdf(o.name)" :path="o.path" :width="34" />
          <img v-else-if="isImage(o.name)" :src="fileUrl(o.path)" alt="" loading="lazy" class="w-[34px] h-[44px] object-cover rounded-sm border border-border dark:border-border-dark" />
          <FileText v-else class="w-[34px] h-6 text-muted" aria-hidden="true" />
          <span class="flex-1 min-w-0 truncate font-medium text-sm text-text dark:text-text-dark">{{ o.name }}</span>
          <span class="text-xs text-muted dark:text-muted-dark tabular-nums">{{ formatSize(o.size, locale) }}</span>
          <button type="button" class="btn-icon" :aria-label="t('utilities.result.open_one', { name: o.name })" :title="t('utilities.result.open')" @click="open(o.name)">
            <ExternalLink class="w-4 h-4" aria-hidden="true" />
          </button>
        </li>
      </ul>
      <p v-if="!job.saved" class="flex items-center gap-1.5 text-xs text-muted dark:text-muted-dark m-0 mt-1">
        <Clock class="w-3.5 h-3.5" aria-hidden="true" />{{ t('utilities.result.not_saved_all', { hours: hoursLeft }, hoursLeft) }}
      </p>
      <div class="flex flex-wrap gap-2 mt-2">
        <button type="button" class="btn-primary" :disabled="busy" @click="saveAll">
          <FolderOpen class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.save_all') }}
        </button>
        <button type="button" class="btn-secondary" :disabled="busy" @click="saveZip">
          <Download class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.save_zip') }}
        </button>
        <button type="button" class="btn-secondary !border-transparent text-muted dark:text-muted-dark" :disabled="busy" @click="throwAway">
          <Trash2 class="w-4 h-4" aria-hidden="true" />{{ t('utilities.result.discard') }}
        </button>
      </div>
    </div>

    <section v-if="next.length" class="mt-6">
      <h3 class="text-sm font-semibold text-text dark:text-text-dark mb-2">{{ t('utilities.result.use_with') }}</h3>
      <div class="flex flex-wrap gap-2">
        <button
          v-for="id in next"
          :key="id"
          type="button"
          class="inline-flex items-center gap-2 h-[34px] pl-1.5 pr-3 rounded-full border border-border dark:border-border-dark bg-surface dark:bg-surface-dark text-sm hover:border-accent"
          @click="emit('chain', id)"
        >
          <span class="w-6 h-6 rounded-full grid place-items-center" :class="`utility-tint-${toolById(id)!.category}`">
            <component :is="toolById(id)!.icon" class="w-3.5 h-3.5" aria-hidden="true" />
          </span>
          {{ t(`utilities.tools.${id}.name`) }}
        </button>
      </div>
      <p class="text-xs text-muted dark:text-muted-dark mt-2 flex items-center gap-1.5">
        <Copy class="w-3.5 h-3.5" aria-hidden="true" />{{ t('utilities.result.use_with_hint') }}
      </p>
    </section>
  </div>
</template>
