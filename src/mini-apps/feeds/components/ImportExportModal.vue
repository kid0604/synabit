<script setup lang="ts">
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Upload, Download, X, FileText, Check, AlertCircle, Loader2 } from 'lucide-vue-next';
import { useArticleService } from '../composables/useArticleService';
import { open, save } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';

const emit = defineEmits<{ close: []; imported: [] }>();
const { t } = useI18n();
const feedService = useArticleService();

const activeTab = ref<'import' | 'export'>('import');
const importing = ref(false);
const exporting = ref(false);
const importResult = ref<{ success: boolean; count: number; error?: string } | null>(null);
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

    await feedService.importOpml(filePath as string);

    importResult.value = { success: true, count: 0 };
    emit('imported');
  } catch (e: any) {
    importResult.value = { success: false, count: 0, error: e?.toString() || 'Import failed' };
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

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') emit('close');
};
</script>

<template>
  <div class="fixed inset-0 z-[200] flex items-center justify-center" @keydown="handleKeydown">
    <!-- Backdrop -->
    <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" @click="emit('close')"></div>

    <!-- Modal -->
    <div class="relative w-full max-w-lg mx-4 bg-white dark:bg-[#1a1a1a] rounded-2xl shadow-2xl border border-gray-200 dark:border-[#2c2c2c] overflow-hidden animate-in">
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-[#2c2c2c]">
        <h2 class="text-lg font-bold flex items-center gap-2">
          <FileText class="w-5 h-5 text-orange-500" />
          {{ t('feeds.import_export_opml') }}
        </h2>
        <button @click="emit('close')" class="p-1.5 rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Tabs -->
      <div class="flex border-b border-gray-200 dark:border-[#2c2c2c]">
        <button
          @click="activeTab = 'import'"
          :class="[
            'flex-1 flex items-center justify-center gap-2 px-4 py-3 text-sm font-medium transition-all duration-200',
            activeTab === 'import'
              ? 'text-orange-600 dark:text-orange-400 border-b-2 border-orange-500'
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
              ? 'text-orange-600 dark:text-orange-400 border-b-2 border-orange-500'
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
            class="border-2 border-dashed border-gray-300 dark:border-gray-600 rounded-xl p-8 text-center hover:border-orange-400 dark:hover:border-orange-500 transition-colors cursor-pointer"
            @click="handleImport"
          >
            <Upload class="w-10 h-10 text-gray-400 dark:text-gray-500 mx-auto mb-3" />
            <p class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{{ t('feeds.browse_file') }}</p>
            <p class="text-xs text-gray-400 dark:text-gray-500">OPML, XML</p>
          </div>

          <!-- Importing spinner -->
          <div v-if="importing" class="flex items-center justify-center gap-2 py-3">
            <Loader2 class="w-5 h-5 animate-spin text-orange-500" />
            <span class="text-sm text-gray-500">{{ t('feeds.importing') }}</span>
          </div>

          <!-- Import result -->
          <div v-if="importResult && importResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400 text-sm">
            <Check class="w-5 h-5 shrink-0" />
            {{ t('feeds.import_success', { count: importResult.count }) }}
          </div>
          <div v-if="importResult && !importResult.success" class="flex items-center gap-2 px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm">
            <AlertCircle class="w-5 h-5 shrink-0" />
            {{ importResult.error || t('feeds.import_error') }}
          </div>
        </div>

        <!-- Export Tab -->
        <div v-if="activeTab === 'export'" class="space-y-4">
          <div class="text-center py-4">
            <Download class="w-10 h-10 text-gray-400 dark:text-gray-500 mx-auto mb-3" />
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">{{ t('feeds.export_opml') }}</p>
            <button
              @click="handleExport"
              :disabled="exporting"
              class="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-orange-500 text-white text-sm font-medium hover:bg-orange-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
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
      <div class="flex items-center justify-end px-6 py-4 border-t border-gray-200 dark:border-[#2c2c2c]">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          {{ t('feeds.cancel') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.animate-in {
  animation: modal-in 0.2s ease-out;
}

@keyframes modal-in {
  from {
    opacity: 0;
    transform: scale(0.95) translateY(10px);
  }
  to {
    opacity: 1;
    transform: scale(1) translateY(0);
  }
}
</style>
