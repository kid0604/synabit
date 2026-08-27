<script setup lang="ts">
/**
 * Tell the user their notes are one uninstall away from gone.
 *
 * On Android the vault lives in storage the operating system deletes with the
 * app. Sync covers whoever turned it on; for everybody else the only copy is on
 * that phone, and nothing in the product ever said so. This is the thing that
 * says so — once when there is no backup at all, and again when the last one
 * has gone stale.
 *
 * Deliberately a banner and not a modal. It appears at startup, which is when
 * the user is trying to get to their notes, and a modal at that moment is
 * something to dismiss rather than something to read.
 */
import { ref, computed, onMounted } from 'vue';
import { HardDrive } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { isMobileOS } from '../platformScope';
import { useVaultArchive, lastExportedAt, backupReminderReason, daysSince as daysSinceExport, formatBytes } from '../../composables/useVaultArchive';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();
const { busy, exportVault } = useVaultArchive();

const reason = ref<ReturnType<typeof backupReminderReason>>(null);
const daysSince = ref(0);
const dismissed = ref(false);
const done = ref('');

const visible = computed(() => !dismissed.value && reason.value !== null && !!props.vaultPath);

onMounted(async () => {
  // Only where uninstalling actually destroys the vault. On the desktop the
  // vault is a folder the user chose and can see; nagging about it there would
  // be noise.
  if (!isMobileOS.value || !props.vaultPath) return;

  const last = await lastExportedAt();
  const now = new Date();
  reason.value = backupReminderReason(last, now, isMobileOS.value);
  if (reason.value === 'stale' && last) {
    daysSince.value = daysSinceExport(last, now);
  }
});

async function runExport() {
  try {
    const summary = await exportVault(props.vaultPath);
    // null means the user closed the dialog, which is not a refusal to ever
    // back up — leave the banner where it is.
    if (!summary) return;
    done.value = t('settings.general.backup_exported', {
      files: summary.files,
      size: formatBytes(summary.bytes),
    });
    setTimeout(() => { dismissed.value = true; }, 2500);
  } catch (e) {
    logger.error('Vault export from the reminder failed', e);
    done.value = String(e);
  }
}
</script>

<template>
  <div v-if="visible"
       class="w-full px-4 py-3 bg-amber-50 dark:bg-amber-950/40 border-b border-amber-200 dark:border-amber-900">
    <!--
      Two rows, not one. A phone is 360dp wide; an icon, a sentence, a button
      and a close control side by side leaves about a hundred pixels for the
      sentence, which is how this first shipped and why it wrapped one word to
      a line. The message gets the full width and the actions sit under it.
    -->
    <div class="flex items-start gap-2.5">
      <HardDrive class="w-4 h-4 mt-0.5 flex-shrink-0 text-amber-700 dark:text-amber-500" />
      <p class="text-[13px] leading-relaxed text-amber-900 dark:text-amber-200 min-w-0">
        <template v-if="done">{{ done }}</template>
        <template v-else-if="reason === 'never'">{{ $t('settings.general.backup_notice_never') }}</template>
        <template v-else>{{ $t('settings.general.backup_notice_stale', { days: daysSince }) }}</template>
      </p>
    </div>

    <div v-if="!done" class="flex items-center justify-end gap-2 mt-2.5">
      <!-- Labelled rather than an X: clearer about what it does, and a target
           a thumb can actually hit. -->
      <button @click="dismissed = true"
              class="min-h-[40px] px-3 rounded-lg text-[13px] font-medium text-amber-800 dark:text-amber-300 hover:bg-amber-100 dark:hover:bg-amber-900/50 transition-colors">
        {{ $t('settings.general.backup_notice_dismiss') }}
      </button>
      <button @click="runExport" :disabled="busy"
              class="min-h-[40px] px-4 rounded-lg text-[13px] font-medium bg-amber-700 hover:bg-amber-800 text-white transition-colors disabled:opacity-50">
        {{ busy ? $t('settings.general.backup_working') : $t('settings.general.backup_export') }}
      </button>
    </div>
  </div>
</template>
