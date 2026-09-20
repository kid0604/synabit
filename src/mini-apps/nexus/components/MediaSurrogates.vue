<script setup lang="ts">
/**
 * The switches for transcripts and captions, inside the extraction tray.
 *
 * Made only on a computer, transcripts only by a server on this machine and
 * captions only by Ollama: Rust refuses anything else, and this says so before
 * anyone asks. See `src-tauri/src/timeline/media.rs`.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Loader2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export interface MediaStatus {
    config: { transcripts: boolean; transcribe_url: string; transcribe_model: string; captions: boolean; caption_model: string };
    desktop: boolean;
    provider: string;
    local_provider: boolean;
    pending_transcripts: number;
    pending_captions: number;
    done: number;
    too_large: number;
    running: boolean;
}

interface MediaRun { read: number; failed: string[]; remaining: number; skipped: string | null }

const props = defineProps<{ vaultPath: string }>();

const { locale } = useI18n();
const FIELD = 'w-full rounded-md border border-gray-200 bg-white px-2 py-1 text-[11px] text-gray-800 placeholder:text-gray-400 focus:border-indigo-400 focus:outline-none dark:border-[#3a3a3c] dark:bg-[#1e1e20] dark:text-gray-200';

const status = ref<MediaStatus | null>(null);
const draft = ref({ transcripts: false, transcribe_url: '', transcribe_model: '', captions: false, caption_model: '' });
const failure = ref('');
const busy = ref(false);
const lastRun = ref<MediaRun | null>(null);

const load = async () => {
    try {
        status.value = await invoke<MediaStatus>('timeline_media_status', { vaultPath: props.vaultPath });
        draft.value = { ...status.value.config };
    } catch (e) {
        logger.error('Could not read the media status', e);
    }
};

onMounted(load);

/**
 * Whether these two settings differ from what is stored.
 *
 * The button sits at the bottom of a long panel whose other content is
 * proposals, and each proposal is written the moment it is kept. Somebody who
 * has just reworded eight of them reaches this and reasonably wonders whether
 * their work is unsaved. A button that is dark when there is nothing to save
 * is the question being asked; one that is plainly inert is the answer.
 */
const changed = computed(() => {
    const stored = status.value?.config;
    if (!stored) return false;
    return (Object.keys(draft.value) as (keyof typeof draft.value)[]).some(
        key => draft.value[key] !== stored[key],
    );
});

const save = async () => {
    failure.value = '';
    try {
        await invoke('timeline_media_configure', { vaultPath: props.vaultPath, settings: draft.value });
        await load();
    } catch (e) {
        failure.value = errorText(e);
    }
};

const run = async () => {
    busy.value = true;
    failure.value = '';
    try {
        lastRun.value = await invoke<MediaRun>('timeline_media_run', {
            vaultPath: props.vaultPath,
            auto: false,
            limit: null,
            locale: locale.value,
        });
        await load();
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        busy.value = false;
    }
};
</script>

<template>
    <section v-if="status" data-media class="space-y-2 border-t border-gray-100 pt-3 dark:border-[#3a3a3c]">
        <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ $t('nexus.media_title') }}</p>
        <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.media_explain') }}</p>

        <p v-if="!status.desktop" data-media-phone class="text-[11px] text-gray-600 dark:text-gray-300">{{ $t('nexus.media_phone') }}</p>

        <template v-else>
            <label class="flex items-center gap-2 text-[11px] font-medium text-gray-700 dark:text-gray-300">
                <input v-model="draft.transcripts" type="checkbox" data-transcripts /> {{ $t('nexus.media_transcripts') }}
            </label>
            <div v-if="draft.transcripts" class="space-y-1 pl-5">
                <input v-model="draft.transcribe_url" type="url" data-transcribe-url :class="FIELD" :placeholder="$t('nexus.media_transcribe_url')" />
                <input v-model="draft.transcribe_model" type="text" :class="FIELD" :placeholder="$t('nexus.media_transcribe_model')" />
                <p class="text-[10px] text-gray-400">{{ $t('nexus.media_transcribe_hint') }}</p>
            </div>

            <label class="flex items-center gap-2 text-[11px] font-medium text-gray-700 dark:text-gray-300">
                <input v-model="draft.captions" type="checkbox" data-captions :disabled="!status.local_provider" /> {{ $t('nexus.media_captions') }}
            </label>
            <p v-if="!status.local_provider" data-caption-cloud class="pl-5 text-[10px] text-amber-700 dark:text-amber-400">
                {{ $t('nexus.media_caption_cloud', { provider: status.provider }) }}
            </p>
            <div v-else-if="draft.captions" class="space-y-1 pl-5">
                <input v-model="draft.caption_model" type="text" :class="FIELD" :placeholder="$t('nexus.media_caption_model')" />
                <p class="text-[10px] text-gray-400">{{ $t('nexus.media_no_faces') }}</p>
            </div>

            <!-- Named for what it saves, and inert when that is nothing. The
                 proposals above are written when they are kept; this button
                 has never had anything to do with them. -->
            <button
                type="button"
                data-media-save
                :disabled="!changed"
                class="rounded-md border border-gray-200 px-2.5 py-1 text-[11px] transition-opacity disabled:cursor-default disabled:opacity-40 dark:border-[#3a3a3c]"
                @click="save"
            >{{ $t('nexus.media_save') }}</button>

            <div v-if="status.config.transcripts || status.config.captions" class="space-y-1 rounded-lg bg-gray-50 p-2.5 text-[11px] text-gray-600 dark:bg-[#1e1e20] dark:text-gray-300">
                <p class="tabular-nums">{{ $t('nexus.media_pending', { transcripts: status.pending_transcripts, captions: status.pending_captions, done: status.done }) }}</p>
                <p v-if="status.too_large">{{ $t('nexus.media_too_large', { count: status.too_large }) }}</p>
                <button
                    type="button"
                    data-media-run
                    class="flex items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1 font-semibold text-white disabled:opacity-40"
                    :disabled="busy || status.running || status.pending_transcripts + status.pending_captions === 0"
                    @click="run"
                >
                    <Loader2 v-if="busy || status.running" class="h-3 w-3 animate-spin" />
                    {{ busy || status.running ? $t('nexus.media_running') : $t('nexus.media_run') }}
                </button>
                <p v-if="lastRun && !lastRun.skipped">{{ $t('nexus.media_last_run', { read: lastRun.read, failed: lastRun.failed.length }) }}</p>
            </div>
        </template>

        <p v-if="failure" class="text-[11px] text-red-500">{{ failure }}</p>
    </section>
</template>
