<script setup lang="ts">
import { ref } from 'vue';
import { Lock, ShieldCheck } from 'lucide-vue-next';
import LockScreen from './LockScreen.vue';
import { useAppLockStore } from '../../stores/useAppLockStore';

const props = defineProps<{
  appId: string;
  appName: string;
}>();

const emit = defineEmits<{
  (e: 'unlocked'): void;
}>();

const store = useAppLockStore();
const showPinOverlay = ref(false);

function requestUnlock() {
  showPinOverlay.value = true;
}

function onUnlocked() {
  showPinOverlay.value = false;
  store.unlockMiniApp(props.appId);
  emit('unlocked');
}
</script>

<template>
  <div class="flex items-center justify-center w-full h-full min-h-[400px] bg-[#fdfdfc] dark:bg-[#242424]">
    <div class="flex flex-col items-center max-w-sm px-6 py-12 text-center">
      <!-- Lock Icon Container -->
      <div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-[#f5f5f5] to-[#e8e8e8] dark:from-[#2a2a2a] dark:to-[#333] flex items-center justify-center mb-5 shadow-inner">
        <Lock class="w-7 h-7 text-[#8b8b8b] dark:text-[#71717a]" :stroke-width="1.5" />
      </div>

      <!-- App Name -->
      <h2 class="text-[17px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-2">
        {{ appName }} is Protected
      </h2>

      <!-- Description -->
      <p class="text-[13px] text-[#8b8b8b] dark:text-[#71717a] mb-8 leading-relaxed">
        This app requires PIN verification to access. Your data stays secure until you unlock it.
      </p>

      <!-- Unlock Button -->
      <button
        @click="requestUnlock"
        class="group flex items-center gap-2.5 px-6 py-3 rounded-xl
          bg-[#1c1c1e] hover:bg-[#333] dark:bg-white dark:hover:bg-gray-200
          text-white dark:text-[#1c1c1e]
          text-[13px] font-semibold
          transition-all duration-200 shadow-sm hover:shadow-md
          active:scale-[0.97]"
      >
        <ShieldCheck class="w-4 h-4 opacity-80 group-hover:opacity-100 transition-opacity" />
        Unlock with PIN
      </button>
    </div>

    <!-- PIN Overlay -->
    <LockScreen
      v-if="showPinOverlay"
      :title="`Enter PIN to access ${appName}`"
      @unlocked="onUnlocked"
      @cancelled="showPinOverlay = false"
    />
  </div>
</template>
