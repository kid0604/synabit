<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Shield, Key, ArrowRight, Copy, Check, AlertCircle } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import AppDialog from './AppDialog.vue';

const { t } = useI18n();

const emit = defineEmits<{
  (e: 'done'): void;
}>();

type Step = 'choose' | 'generate' | 'restore' | 'show-phrase' | 'verify-phrase';
const step = ref<Step>('choose');
const loading = ref(false);
const error = ref('');
const recoveryPhrase = ref('');
const restoreInput = ref('');
const verifyInput = ref('');
const copied = ref(false);

const generateNew = async () => {
  loading.value = true;
  error.value = '';
  try {
    const result = await invoke<{ recovery_phrase: string }>('setup_e2ee');
    recoveryPhrase.value = result.recovery_phrase;
    step.value = 'show-phrase';
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
};

const restoreFromPhrase = async () => {
  if (!restoreInput.value.trim()) {
    error.value = t('settings.e2ee.enter_phrase');
    return;
  }
  loading.value = true;
  error.value = '';
  try {
    await invoke('restore_e2ee_from_phrase', { phrase: restoreInput.value.trim().toLowerCase() });
    emit('done');
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
};

const copyPhrase = () => {
  navigator.clipboard.writeText(recoveryPhrase.value);
  copied.value = true;
  setTimeout(() => { copied.value = false; }, 2000);
};

const startVerify = () => {
  step.value = 'verify-phrase';
  error.value = '';
  verifyInput.value = '';
};

const finishSetup = () => {
  if (verifyInput.value.trim().toLowerCase() !== recoveryPhrase.value.trim().toLowerCase()) {
    error.value = t('settings.e2ee.mismatch');
    return;
  }
  emit('done');
};
</script>

<template>
  <!--
    Not dismissible: sync cannot start until a key exists, so there is no
    "later" to close it into. Elevated, because it opens over everything.
  -->
  <AppDialog show labelledby="e2ee-onboarding-title" :dismissible="false" elevated unstyled>
    <div class="bg-white dark:bg-[#1c1c1e] rounded-2xl shadow-2xl w-full overflow-hidden" @mousedown.stop>
      
      <!-- Header -->
      <div class="px-6 pt-6 pb-4 text-center">
        <div class="w-14 h-14 bg-accent/10 rounded-2xl flex items-center justify-center mx-auto mb-4">
          <Shield class="w-7 h-7 text-accent dark:text-accent-dark" />
        </div>
        <h2 id="e2ee-onboarding-title" class="text-xl font-bold text-text dark:text-text-dark">
          {{ step === 'show-phrase' ? $t('settings.e2ee.save_title') : $t('settings.e2ee.title') }}
        </h2>
        <p class="text-[13px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed">
          <template v-if="step === 'choose'">
            {{ $t('settings.e2ee.choose_desc') }}
          </template>
          <template v-else-if="step === 'generate'">
            {{ $t('settings.e2ee.generate_desc') }}
          </template>
          <template v-else-if="step === 'restore'">
            {{ $t('settings.e2ee.restore_desc') }}
          </template>
          <template v-else-if="step === 'show-phrase'">
            <i18n-t keypath="settings.e2ee.show_desc" tag="span"><template #warning><strong class="text-red-500">{{ $t('settings.e2ee.save_it') }}</strong></template></i18n-t>
          </template>
          <template v-else-if="step === 'verify-phrase'">
            {{ $t('settings.e2ee.verify_desc') }}
          </template>
        </p>
      </div>

      <!-- Content -->
      <div class="px-6 pb-6">
        
        <!-- Step: Choose -->
        <div v-if="step === 'choose'" class="space-y-3">
          <button @click="step = 'generate'" class="w-full p-4 rounded-xl border-2 border-border dark:border-[#333] hover:border-accent dark:hover:border-accent-dark bg-[#f8f8f8] dark:bg-[#252525] transition-all group text-left flex items-start gap-3">
            <div class="w-10 h-10 rounded-lg bg-accent/10 flex items-center justify-center shrink-0 group-hover:bg-accent/20 transition-colors">
              <Key class="w-5 h-5 text-accent dark:text-accent-dark" />
            </div>
            <div>
              <p class="text-[14px] font-semibold text-text dark:text-text-dark">{{ $t('settings.e2ee.first_device') }}</p>
              <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.e2ee.first_device_desc') }}</p>
            </div>
            <ArrowRight class="w-4 h-4 text-gray-500 dark:text-gray-400 ml-auto mt-3" />
          </button>
          
          <button @click="step = 'restore'" class="w-full p-4 rounded-xl border-2 border-border dark:border-[#333] hover:border-accent dark:hover:border-accent-dark bg-[#f8f8f8] dark:bg-[#252525] transition-all group text-left flex items-start gap-3">
            <div class="w-10 h-10 rounded-lg bg-accent/10 flex items-center justify-center shrink-0 group-hover:bg-accent/20 transition-colors">
              <ArrowRight class="w-5 h-5 text-accent dark:text-accent-dark" />
            </div>
            <div>
              <p class="text-[14px] font-semibold text-text dark:text-text-dark">{{ $t('settings.e2ee.existing') }}</p>
              <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.e2ee.existing_desc') }}</p>
            </div>
            <ArrowRight class="w-4 h-4 text-gray-500 dark:text-gray-400 ml-auto mt-3" />
          </button>
        </div>

        <!-- Step: Generate -->
        <div v-else-if="step === 'generate'" class="space-y-4">
          <div class="bg-accent/5 border border-accent/20 p-3 rounded-lg">
            <p class="text-[12px] text-text dark:text-text-dark leading-relaxed">
              {{ $t('settings.e2ee.generate_info') }}
            </p>
          </div>
          <button @click="generateNew" :disabled="loading" class="btn-primary w-full h-11">
            <Key class="w-4 h-4" />
            {{ loading ? $t('settings.e2ee.generating') : $t('settings.e2ee.generate') }}
          </button>
          <button @click="step = 'choose'; error = ''" class="w-full px-4 py-2 text-[13px] text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
            ← {{ $t('shell.common.back') }}
          </button>
        </div>

        <!-- Step: Restore -->
        <div v-else-if="step === 'restore'" class="space-y-4">
          <div class="space-y-1.5">
            <label for="e2ee-restore" class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.security.recovery_phrase') }}</label>
            <textarea 
              id="e2ee-restore"
              v-model="restoreInput" 
              rows="3" 
              :placeholder="$t('settings.e2ee.restore_placeholder')" 
              class="w-full px-3 py-2.5 rounded-xl bg-[#f8f8f8] dark:bg-[#252525] border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-accent resize-none font-mono"
            ></textarea>
          </div>
          <button @click="restoreFromPhrase" :disabled="loading" class="btn-primary w-full h-11">
            {{ loading ? $t('settings.security.restoring') : $t('settings.security.restore') }}
          </button>
          <button @click="step = 'choose'; error = ''; restoreInput = ''" class="w-full px-4 py-2 text-[13px] text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
            ← {{ $t('shell.common.back') }}
          </button>
        </div>

        <!-- Step: Show Recovery Phrase (only time it's shown) -->
        <div v-else-if="step === 'show-phrase'" class="space-y-4">
          <div class="bg-amber-50 dark:bg-amber-900/20 border border-amber-300 dark:border-amber-700 p-3 rounded-lg">
            <p class="text-xs text-amber-700 dark:text-amber-400 font-medium">
              ⚠ {{ $t('settings.e2ee.only_once') }}
            </p>
          </div>

          <div class="bg-[#f8f8f8] dark:bg-[#252525] p-4 rounded-xl border border-border dark:border-border-subtle-dark">
            <p class="font-mono text-[14px] text-text dark:text-text-dark leading-relaxed select-all break-words text-center">
              {{ recoveryPhrase }}
            </p>
          </div>
          
          <button @click="copyPhrase" class="w-full px-4 py-2.5 border border-border-subtle dark:border-border-subtle-dark text-text-secondary dark:text-text-secondary-dark hover:bg-gray-100 dark:hover:bg-[#333] rounded-xl text-[13px] font-medium transition-all flex items-center justify-center gap-2">
            <Copy v-if="!copied" class="w-4 h-4" />
            <Check v-else class="w-4 h-4 text-green-500" />
            {{ copied ? $t('settings.pairing.copied') : $t('settings.e2ee.copy_phrase') }}
          </button>
          
          <button @click="startVerify" class="btn-primary w-full h-11">
            {{ $t('settings.e2ee.saved_continue') }}
          </button>
        </div>

        <!-- Step: Verify Phrase -->
        <div v-else-if="step === 'verify-phrase'" class="space-y-4">
          <div class="space-y-1.5">
            <label for="e2ee-verify" class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.e2ee.verify_label') }}</label>
            <textarea 
              id="e2ee-verify"
              v-model="verifyInput" 
              rows="3" 
              :placeholder="$t('settings.e2ee.verify_placeholder')" 
              class="w-full px-3 py-2.5 rounded-xl bg-[#f8f8f8] dark:bg-[#252525] border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-accent resize-none font-mono"
            ></textarea>
          </div>
          
          <div class="flex gap-3">
            <button @click="step = 'show-phrase'; error = ''" class="flex-1 px-4 py-3 border border-border-subtle dark:border-border-subtle-dark text-text-secondary dark:text-text-secondary-dark hover:bg-gray-100 dark:hover:bg-[#333] rounded-xl text-[14px] font-semibold transition-all">
              {{ $t('shell.common.back') }}
            </button>
            <button @click="finishSetup" class="btn-primary flex-[2] h-auto py-3">
              {{ $t('settings.e2ee.confirm_finish') }}
            </button>
          </div>
        </div>

        <!-- Error -->
        <div v-if="error" class="mt-4 p-3 bg-red-50 dark:bg-red-900/20 rounded-lg flex items-start gap-2">
          <AlertCircle class="w-4 h-4 text-red-500 shrink-0 mt-0.5" />
          <p class="text-[12px] text-red-600 dark:text-red-400 font-medium">{{ error }}</p>
        </div>
      </div>
    </div>
  </AppDialog>
</template>
