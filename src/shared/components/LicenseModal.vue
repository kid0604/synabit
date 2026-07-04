<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { X, Key, Zap, Shield, CheckCircle2, AlertCircle, RefreshCw, TerminalSquare } from 'lucide-vue-next';
import { useLicenseStore } from '../../stores/useLicenseStore';

const props = defineProps<{
    isOpen: boolean;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
}>();

const licenseStore = useLicenseStore();
const licenseKey = ref('');
const errorText = ref('');
const successText = ref('');

onMounted(async () => {
    if (!licenseStore.isReady) {
        await licenseStore.checkState();
    }
});

const handleActivateTrial = async () => {
    errorText.value = '';
    successText.value = '';
    const success = await licenseStore.activateTrial();
    if (success) {
        successText.value = 'Trial activated successfully!';
        setTimeout(() => emit('close'), 1500);
    } else {
        errorText.value = licenseStore.errorMsg || 'Failed to activate trial';
    }
};

const handleActivateKey = async () => {
    if (!licenseKey.value.trim()) {
        errorText.value = 'Please enter a license key';
        return;
    }
    errorText.value = '';
    successText.value = '';
    const success = await licenseStore.activateKey(licenseKey.value.trim());
    if (success) {
        successText.value = 'License activated successfully!';
        setTimeout(() => emit('close'), 1500);
    } else {
        errorText.value = licenseStore.errorMsg || 'Invalid or revoked license key';
    }
};

const handleDeactivate = async () => {
    if (confirm("Are you sure you want to deactivate this device? You will lose access to Pro features.")) {
        await licenseStore.deactivate();
    }
};

const handleRefresh = async () => {
    await licenseStore.refresh();
};
</script>

<template>
  <div v-if="isOpen" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/50 backdrop-blur-sm p-4">
    <div class="bg-surface dark:bg-surface-dark w-full max-w-lg rounded-2xl shadow-xl overflow-hidden animate-in fade-in zoom-in-95 duration-200">
      
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-border dark:border-border-dark">
        <h2 class="text-xl font-semibold text-text dark:text-text-dark flex items-center gap-2">
            <Shield class="w-5 h-5 text-primary" />
            Synabit License
        </h2>
        <button @click="emit('close')" class="p-2 rounded-lg hover:bg-hover dark:hover:bg-hover-dark text-text-muted dark:text-text-muted-dark transition-colors">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-6 space-y-6">
        
        <!-- Current Status -->
        <div class="p-4 rounded-xl border border-border dark:border-border-dark bg-background dark:bg-background-dark relative overflow-hidden">
            <div class="absolute right-0 top-0 w-32 h-32 bg-primary/10 rounded-bl-full -z-0"></div>
            
            <div class="flex items-start justify-between relative z-10">
                <div>
                    <h3 class="text-sm font-medium text-text-muted dark:text-text-muted-dark uppercase tracking-wider mb-1">Current Status</h3>
                    
                    <div v-if="licenseStore.isLoading" class="flex items-center gap-2 text-primary font-medium">
                        <RefreshCw class="w-5 h-5 animate-spin" /> Checking...
                    </div>
                    <div v-else-if="licenseStore.isPro" class="flex items-center gap-2 text-green-600 dark:text-green-400 font-semibold text-lg">
                        <CheckCircle2 class="w-6 h-6" /> Pro Active
                    </div>
                    <div v-else-if="licenseStore.isTrial" class="flex items-center gap-2 text-orange-500 font-semibold text-lg">
                        <Zap class="w-6 h-6" /> Trial Active ({{ licenseStore.daysLeft }} days left)
                    </div>
                    <div v-else-if="licenseStore.isDev" class="flex items-center gap-2 text-blue-500 font-semibold text-lg">
                        <TerminalSquare class="w-6 h-6" /> Developer Mode
                    </div>
                    <div v-else class="flex items-center gap-2 text-red-500 font-semibold text-lg">
                        <AlertCircle class="w-6 h-6" /> {{ licenseStore.licenseStatus.type === 'NoLicense' ? 'Not Activated' : licenseStore.licenseStatus.type }}
                    </div>
                </div>

                <!-- Refresh Button -->
                <button v-if="licenseStore.licenseStatus.type !== 'NoLicense'" @click="handleRefresh" class="p-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark hover:bg-hover dark:hover:bg-hover-dark transition-colors" title="Refresh License">
                    <RefreshCw class="w-4 h-4 text-text-muted dark:text-text-muted-dark" :class="{ 'animate-spin': licenseStore.isLoading }" />
                </button>
            </div>

            <div v-if="licenseStore.licenseStatus.type === 'Active'" class="mt-4 pt-4 border-t border-border dark:border-border-dark text-sm text-text-muted dark:text-text-muted-dark">
                <div class="flex justify-between mb-1">
                    <span>License Key:</span>
                    <span class="font-mono text-text dark:text-text-dark">{{ licenseStore.licenseStatus.data.license_key.replace(/^(.{8}).*(.{4})$/, '$1****$2') }}</span>
                </div>
                <div class="flex justify-between mb-1">
                    <span>Expires:</span>
                    <span class="text-text dark:text-text-dark">{{ new Date(licenseStore.licenseStatus.data.expires_at).toLocaleDateString() }}</span>
                </div>
                <div class="flex justify-between">
                    <span>Device Limit:</span>
                    <span class="text-text dark:text-text-dark">{{ licenseStore.licenseStatus.data.max_devices }} devices</span>
                </div>
            </div>

            <!-- Read Only Warning -->
            <div v-if="licenseStore.isReadOnly && licenseStore.licenseStatus.type !== 'NoLicense'" class="mt-4 p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm flex gap-2">
                <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
                <span>Your app is in Read-Only mode. Please activate a valid license to restore editing capabilities.</span>
            </div>
        </div>

        <!-- Activation Forms -->
        <div v-if="!licenseStore.isPro">
            
            <div class="space-y-4">
                <div v-if="licenseStore.licenseStatus.type === 'NoLicense'">
                    <button 
                        @click="handleActivateTrial" 
                        :disabled="licenseStore.isLoading"
                        class="w-full py-3 px-4 bg-surface dark:bg-surface-dark hover:bg-hover dark:hover:bg-hover-dark border border-border dark:border-border-dark rounded-xl font-medium text-text dark:text-text-dark flex items-center justify-center gap-2 transition-colors disabled:opacity-50"
                    >
                        <Zap class="w-5 h-5 text-orange-500" />
                        Start 100-Day Free Trial
                    </button>

                    <div class="relative my-6">
                        <div class="absolute inset-0 flex items-center"><div class="w-full border-t border-border dark:border-border-dark"></div></div>
                        <div class="relative flex justify-center text-sm"><span class="px-2 bg-surface dark:bg-surface-dark text-text-muted dark:text-text-muted-dark">Or activate with key</span></div>
                    </div>
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-muted dark:text-text-muted-dark mb-1">License Key</label>
                    <div class="flex gap-2">
                        <div class="relative flex-1">
                            <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                                <Key class="w-4 h-4 text-text-muted dark:text-text-muted-dark" />
                            </div>
                            <input 
                                v-model="licenseKey" 
                                type="text" 
                                placeholder="SYN-..." 
                                class="w-full pl-10 pr-4 py-2.5 bg-background dark:bg-background-dark border border-border dark:border-border-dark rounded-xl focus:ring-2 focus:ring-primary focus:outline-none text-text dark:text-text-dark font-mono uppercase"
                                @keyup.enter="handleActivateKey"
                            />
                        </div>
                        <button 
                            @click="handleActivateKey" 
                            :disabled="licenseStore.isLoading || !licenseKey"
                            class="px-6 py-2.5 bg-primary hover:bg-primary-dark text-white rounded-xl font-medium transition-colors disabled:opacity-50"
                        >
                            Activate
                        </button>
                    </div>
                    
                    <p class="text-xs text-text-muted dark:text-text-muted-dark mt-2">
                        Don't have a key? <a href="https://synabit.io/pricing" target="_blank" class="text-primary hover:underline">Purchase one here</a>.
                    </p>
                </div>
            </div>

        </div>

        <!-- Feedback Messages -->
        <div v-if="errorText" class="p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 rounded-lg text-sm flex gap-2 items-start">
            <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
            <span>{{ errorText }}</span>
        </div>
        <div v-if="successText" class="p-3 bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400 rounded-lg text-sm flex gap-2 items-start">
            <CheckCircle2 class="w-4 h-4 shrink-0 mt-0.5" />
            <span>{{ successText }}</span>
        </div>

      </div>

      <!-- Footer Actions -->
      <div v-if="licenseStore.licenseStatus.type !== 'NoLicense'" class="px-6 py-4 bg-background dark:bg-background-dark border-t border-border dark:border-border-dark flex justify-between items-center">
          <button 
            @click="handleDeactivate" 
            class="text-sm font-medium text-red-500 hover:text-red-600 dark:hover:text-red-400 transition-colors"
          >
            Deactivate Device
          </button>
          <span class="text-xs text-text-muted dark:text-text-muted-dark">Unlinks this device from your license</span>
      </div>

    </div>
  </div>
</template>
