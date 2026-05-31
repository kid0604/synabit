<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Shield, Key, ArrowRight, Copy, Check, AlertCircle } from 'lucide-vue-next';

const emit = defineEmits<{
  (e: 'done'): void;
}>();

type Step = 'choose' | 'generate' | 'restore' | 'show-phrase';
const step = ref<Step>('choose');
const loading = ref(false);
const error = ref('');
const recoveryPhrase = ref('');
const restoreInput = ref('');
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
    error.value = 'Please enter your Recovery Phrase';
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

const finishSetup = () => {
  emit('done');
};
</script>

<template>
  <div class="fixed inset-0 z-[9999] bg-black/70 backdrop-blur-md flex items-center justify-center p-4" @mousedown.stop>
    <div class="bg-white dark:bg-[#1c1c1e] rounded-2xl shadow-2xl w-full max-w-md overflow-hidden">
      
      <!-- Header -->
      <div class="px-6 pt-6 pb-4 text-center">
        <div class="w-14 h-14 bg-blue-100 dark:bg-blue-900/30 rounded-2xl flex items-center justify-center mx-auto mb-4">
          <Shield class="w-7 h-7 text-blue-600 dark:text-blue-400" />
        </div>
        <h2 class="text-xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">
          {{ step === 'show-phrase' ? 'Save Your Recovery Phrase' : 'Secure Your Data' }}
        </h2>
        <p class="text-[13px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed">
          <template v-if="step === 'choose'">
            Your data is encrypted before syncing. No one — not even your cloud provider — can read it.
          </template>
          <template v-else-if="step === 'generate'">
            Generate a new encryption key for this device.
          </template>
          <template v-else-if="step === 'restore'">
            Enter the Recovery Phrase from your existing device to sync your data.
          </template>
          <template v-else-if="step === 'show-phrase'">
            This is your Recovery Phrase. <strong class="text-red-500">Save it somewhere safe!</strong> You'll need it to restore data on a new device. This is the only time it will be shown.
          </template>
        </p>
      </div>

      <!-- Content -->
      <div class="px-6 pb-6">
        
        <!-- Step: Choose -->
        <div v-if="step === 'choose'" class="space-y-3">
          <button @click="step = 'generate'" class="w-full p-4 rounded-xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-blue-500 dark:hover:border-blue-400 bg-[#f8f8f8] dark:bg-[#252525] transition-all group text-left flex items-start gap-3">
            <div class="w-10 h-10 rounded-lg bg-blue-100 dark:bg-blue-900/30 flex items-center justify-center shrink-0 group-hover:bg-blue-200 dark:group-hover:bg-blue-900/50 transition-colors">
              <Key class="w-5 h-5 text-blue-600 dark:text-blue-400" />
            </div>
            <div>
              <p class="text-[14px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">First device</p>
              <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-0.5">Create a new encryption key</p>
            </div>
            <ArrowRight class="w-4 h-4 text-gray-400 ml-auto mt-3" />
          </button>
          
          <button @click="step = 'restore'" class="w-full p-4 rounded-xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-green-500 dark:hover:border-green-400 bg-[#f8f8f8] dark:bg-[#252525] transition-all group text-left flex items-start gap-3">
            <div class="w-10 h-10 rounded-lg bg-green-100 dark:bg-green-900/30 flex items-center justify-center shrink-0 group-hover:bg-green-200 dark:group-hover:bg-green-900/50 transition-colors">
              <ArrowRight class="w-5 h-5 text-green-600 dark:text-green-400" />
            </div>
            <div>
              <p class="text-[14px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Existing vault</p>
              <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-0.5">Enter Recovery Phrase from another device</p>
            </div>
            <ArrowRight class="w-4 h-4 text-gray-400 ml-auto mt-3" />
          </button>
        </div>

        <!-- Step: Generate -->
        <div v-else-if="step === 'generate'" class="space-y-4">
          <div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 p-3 rounded-lg">
            <p class="text-[12px] text-blue-700 dark:text-blue-300 leading-relaxed">
              An encryption key will be generated and stored securely on your device. You'll receive a 12-word Recovery Phrase as a backup.
            </p>
          </div>
          <button @click="generateNew" :disabled="loading" class="w-full px-4 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-xl text-[14px] font-semibold transition-all shadow-sm flex items-center justify-center gap-2 disabled:opacity-60">
            <Key class="w-4 h-4" />
            {{ loading ? 'Generating...' : 'Generate Encryption Key' }}
          </button>
          <button @click="step = 'choose'; error = ''" class="w-full px-4 py-2 text-[13px] text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
            ← Back
          </button>
        </div>

        <!-- Step: Restore -->
        <div v-else-if="step === 'restore'" class="space-y-4">
          <div class="space-y-1.5">
            <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Recovery Phrase (12 words)</label>
            <textarea 
              v-model="restoreInput" 
              rows="3" 
              placeholder="word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12" 
              class="w-full px-3 py-2.5 rounded-xl bg-[#f8f8f8] dark:bg-[#252525] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none font-mono"
            ></textarea>
          </div>
          <button @click="restoreFromPhrase" :disabled="loading" class="w-full px-4 py-3 bg-green-600 hover:bg-green-700 text-white rounded-xl text-[14px] font-semibold transition-all shadow-sm flex items-center justify-center gap-2 disabled:opacity-60">
            {{ loading ? 'Restoring...' : 'Restore' }}
          </button>
          <button @click="step = 'choose'; error = ''; restoreInput = ''" class="w-full px-4 py-2 text-[13px] text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
            ← Back
          </button>
        </div>

        <!-- Step: Show Recovery Phrase (only time it's shown) -->
        <div v-else-if="step === 'show-phrase'" class="space-y-4">
          <div class="bg-amber-50 dark:bg-amber-900/20 border border-amber-300 dark:border-amber-700 p-3 rounded-lg">
            <p class="text-[11px] text-amber-700 dark:text-amber-400 font-medium">
              ⚠ This is the only time your Recovery Phrase will be displayed. It cannot be retrieved later.
            </p>
          </div>

          <div class="bg-[#f8f8f8] dark:bg-[#252525] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#3a3a3a]">
            <p class="font-mono text-[14px] text-[#1c1c1e] dark:text-[#f4f4f5] leading-relaxed select-all break-words text-center">
              {{ recoveryPhrase }}
            </p>
          </div>
          
          <button @click="copyPhrase" class="w-full px-4 py-2.5 border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] rounded-xl text-[13px] font-medium transition-all flex items-center justify-center gap-2">
            <Copy v-if="!copied" class="w-4 h-4" />
            <Check v-else class="w-4 h-4 text-green-500" />
            {{ copied ? 'Copied!' : 'Copy Recovery Phrase' }}
          </button>
          
          <button @click="finishSetup" class="w-full px-4 py-3 bg-blue-600 hover:bg-blue-700 text-white rounded-xl text-[14px] font-semibold transition-all shadow-sm">
            I've saved it → Continue
          </button>
        </div>

        <!-- Error -->
        <div v-if="error" class="mt-4 p-3 bg-red-50 dark:bg-red-900/20 rounded-lg flex items-start gap-2">
          <AlertCircle class="w-4 h-4 text-red-500 shrink-0 mt-0.5" />
          <p class="text-[12px] text-red-600 dark:text-red-400 font-medium">{{ error }}</p>
        </div>
      </div>
    </div>
  </div>
</template>
