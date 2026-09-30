<script setup lang="ts">
import { ref } from 'vue';
import { useFocusTrap } from '../composables/useFocusTrap';
import { useI18n } from 'vue-i18n';
import AppDialog from '../../../shared/components/AppDialog.vue';
import { Upload, Download, X, FileText, Check, AlertCircle, Loader2 } from 'lucide-vue-next';
import { useArticleService } from '../composables/useArticleService';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';

const emit = defineEmits<{ close: []; imported: [] }>();
const { t } = useI18n();
const feedService = useArticleService();

// AppDialog moves focus in and handles Escape; the trap keeps Tab inside and
// hands focus back when the dialog is unmounted rather than closed.
const dialog = ref<HTMLElement | null>(null);
useFocusTrap(dialog);

const activeTab = ref<'import' | 'export'>('import');
const importing = ref(false);
const exporting = ref(false);
const importResult = ref<{ success: boolean; count: number; skipped: number; error?: string } | null>(null);
const exportResult = ref<{ success: boolean; error?: string } | null>(null);

const handleImport = async () => {
  try {
    const filePath = await open({
      title: 'Import OPML',
      filters: [{ name: 'OPML', extensions: ['opml', 'xml'] }],
    });
    if (!filePath) return;

    importing.value = true;
    importResult.value = null;

    // The dialog hands back a path; the command parses OPML. Passing the path
    // where the document belonged meant every import failed to parse and then
    // reported success, because nothing checked what came back.
    const opmlContent = await readTextFile(filePath as string);
    const result = await feedService.importOpml(opmlContent);

    importResult.value = { success: true, count: result.added, skipped: result.skipped };
    emit('imported');
  } catch (e: any) {
    importResult.value = { success: false, count: 0, skipped: 0, error: e?.toString() || 'Import failed' };
  } finally {
    importing.value = false;
  }
};

const handleExport = async () => {
  try {
    const filePath = await save({
      title: 'Export OPML',
      defaultPath: 'synabit-feeds.opml',
      filters: [{ name: 'OPML', extensions: ['opml'] }],
    });
    if (!filePath) return;

    exporting.value = true;
    exportResult.value = null;

    const opmlContent = await feedService.exportOpml();
    await writeTextFile(filePath, opmlContent);

    exportResult.value = { success: true };
  } catch (e: any) {
    exportResult.value = { success: false, error: e?.toString() || 'Export failed' };
  } finally {
    exporting.value = false;
  }
};

</script>

<template>
  <AppDialog :show="true" :aria-label="t('feeds.import_export_opml')" size="md" @close="emit('close')">
    <div ref="dialog">
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-border-dark">
        <h2 class="text-lg font-bold flex items-center gap-2">
          <FileText class="w-5 h-5 text-accent dark:text-accent-dark" />
          {{ t('feeds.import_export_opml') }}
        </h2>
        <button @click="emit('close')" class="p-1.5 rounded-lg text-gray-500 dark:text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="t('feeds.a11y_close')">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Tabs -->
      <div class="flex border-b border-gray-200 dark:border-border-dark">
        <button
          @click="activeTab = 'import'"
          :class="[
            'flex-1 flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium transition-all duration-200',
            activeTab === 'import'
              ? 'text-accent dark:text-accent-dark border-b-2 border-accent'
              : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'
          ]"
        >
          <Upload class="w-4 h-4" />
          {{ t('feeds.import_opml') }}
        </button>
        <button
          @click="activeTab = 'export'"
          :class="[
            'flex-1 flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium transition-all duration-200',
            activeTab === 'export'
              ? 'text-accent dark:text-accent-dark border-b-2 border-accent'
              : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300'
          ]"
        >
          <Download class="w-4 h-4" />
          {{ t('feeds.export_opml') }}
        </button>
      </div>

      <!-- Body -->
      <div class="px-6 py-6">
        <!-- Import Tab -->
        <div v-if="activeTab === 'import'" class="space-y-4">
          <div
            class="border-2 border-dashed border-gray-300 dark:border-gray-600 rounded-xl p-8 text-center hover:border-accent transition-colors cursor-pointer"
            @click="handleImport"
          >
            <Upload class="w-10 h-10 text-gray-500 dark:text-gray-400 mx-auto mb-3" />
            <p class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ t('feeds.browse_file') }}</p>
            <p class="text-xs text-gray-500 dark:text-gray-400">OPML, XML</p>
          </div>

          <!-- Importing spinner -->
          <div v-if="importing" class="flex items-center justify-center gap-2 py-3">
            <Loader2 class="w-5 h-5 animate-spin text-accent dark:text-accent-dark" />
            <span class="text-sm text-gray-500 dark:text-gray-400">{{ t('feeds.importing') }}</span>
          </div>

          <!-- Import result -->
          <div v-if="importResult && importResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400 text-sm">
            <Check class="w-5 h-5 shrink-0" />
            <span>
              {{ t('feeds.import_success', { count: importResult.count }) }}
              <template v-if="importResult.skipped > 0">
                · {{ t('feeds.import_skipped', { count: importResult.skipped }) }}
              </template>
            </span>
          </div>
          <div v-if="importResult && !importResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm">
            <AlertCircle class="w-5 h-5 shrink-0" />
            {{ importResult.error || t('feeds.import_error') }}
          </div>
        </div>

        <!-- Export Tab -->
        <div v-if="activeTab === 'export'" class="space-y-4">
          <div class="text-center py-4">
            <Download class="w-10 h-10 text-gray-500 dark:text-gray-400 mx-auto mb-3" />
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">{{ t('feeds.export_opml') }}</p>
            <button
              @click="handleExport"
              :disabled="exporting"
              class="btn-primary"
            >
              <Loader2 v-if="exporting" class="w-4 h-4 animate-spin" />
              <Download v-else class="w-4 h-4" />
              {{ exporting ? t('feeds.exporting') : t('feeds.export_opml') }}
            </button>
          </div>

          <!-- Export result -->
          <div v-if="exportResult && exportResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400 text-sm">
            <Check class="w-5 h-5 shrink-0" />
            {{ t('feeds.export_success') }}
          </div>
          <div v-if="exportResult && !exportResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm">
            <AlertCircle class="w-5 h-5 shrink-0" />
            {{ exportResult.error || t('feeds.export_error') }}
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-end px-6 py-4 border-t border-gray-200 dark:border-border-dark">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          {{ t('feeds.cancel') }}
        </button>
      </div>
    </div>
  </AppDialog>
</template>

