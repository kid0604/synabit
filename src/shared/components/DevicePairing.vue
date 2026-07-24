<script setup lang="ts">
import { ref, watch, nextTick } from 'vue';
import { X, Smartphone, QrCode, Keyboard, Loader2, Check, AlertCircle, Copy, Clock } from 'lucide-vue-next';
import { useDevicePairing } from '../../composables/useDevicePairing';

const props = defineProps<{
  show: boolean;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'paired'): void;
}>();

const {
  isPairing, pairingCode, pairingSuccess, error,
  countdown, countdownFormatted,
  initiatePairing, acceptPairing, cancelPairing,
} = useDevicePairing();

// UI State
type PairingStep = 'choose' | 'generate' | 'enter';
const step = ref<PairingStep>('choose');
const joinCode = ref('');
const joinCodeParts = ref(['', '', '', '', '', '', '', '']);
const joinInputRefs = ref<HTMLInputElement[]>([]);
const isJoining = ref(false);
const codeCopied = ref(false);

// Reset when modal opens/closes
watch(() => props.show, (visible) => {
  if (visible) {
    step.value = 'choose';
    joinCode.value = '';
    joinCodeParts.value = ['', '', '', '', '', '', '', ''];
    error.value = '';
    pairingSuccess.value = false;
    codeCopied.value = false;
  } else {
    cancelPairing();
  }
});

// --- Generate code flow ---
const handleGenerate = async () => {
  step.value = 'generate';
  await initiatePairing();
};

// --- Enter code flow ---
const handleEnterMode = () => {
  step.value = 'enter';
  error.value = '';
  nextTick(() => {
    joinInputRefs.value[0]?.focus();
  });
};

// Handle code input (auto-advance between inputs)
const handleCodeInput = (index: number, event: Event) => {
  const input = event.target as HTMLInputElement;
  const value = input.value.toUpperCase().replace(/[^A-Z0-9]/g, '');

  if (value.length > 1) {
    // Handle paste: distribute characters across inputs
    const chars = value.split('');
    for (let i = 0; i < chars.length && index + i < 8; i++) {
      joinCodeParts.value[index + i] = chars[i];
    }
    const nextIndex = Math.min(index + chars.length, 7);
    joinInputRefs.value[nextIndex]?.focus();
  } else {
    joinCodeParts.value[index] = value;
    if (value && index < 7) {
      joinInputRefs.value[index + 1]?.focus();
    }
  }
};

const handleCodeKeydown = (index: number, event: KeyboardEvent) => {
  if (event.key === 'Backspace' && !joinCodeParts.value[index] && index > 0) {
    joinInputRefs.value[index - 1]?.focus();
  }
  if (event.key === 'Enter') {
    handleJoin();
  }
};

const fullJoinCode = () => {
  const code = joinCodeParts.value.join('');
  return code.slice(0, 4) + '-' + code.slice(4);
};

const handleJoin = async () => {
  const code = fullJoinCode();
  if (code.replace('-', '').length !== 8) {
    error.value = 'Please enter all 8 characters';
    return;
  }
  isJoining.value = true;
  error.value = '';
  await acceptPairing(code);
  isJoining.value = false;
  if (pairingSuccess.value) {
    emit('paired');
  }
};

const handleCopyCode = async () => {
  try {
    await navigator.clipboard.writeText(pairingCode.value);
    codeCopied.value = true;
    setTimeout(() => { codeCopied.value = false; }, 2000);
  } catch (e) {
    // Fallback: select text
  }
};

const handleBack = () => {
  cancelPairing();
  step.value = 'choose';
  joinCode.value = '';
  joinCodeParts.value = ['', '', '', '', '', '', '', ''];
  error.value = '';
  isJoining.value = false;
};

const handleClose = () => {
  cancelPairing();
  emit('close');
};
</script>

<template>
  <Teleport to="body">
    <Transition name="pairing-modal">
      <div
        v-if="show"
        class="fixed inset-0 z-[10000] flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
        @click.self="handleClose"
      >
        <div class="bg-[#fdfdfc] dark:bg-[#242424] w-full max-w-md rounded-2xl shadow-2xl border border-[#e6e6e6] dark:border-[#333] overflow-hidden flex flex-col animate-in">
          <!-- Header -->
          <div class="flex items-center justify-between p-5 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
            <div class="flex items-center gap-3">
              <div class="w-9 h-9 rounded-xl bg-emerald-100 dark:bg-emerald-900/30 flex items-center justify-center">
                <Smartphone class="w-4.5 h-4.5 text-emerald-600 dark:text-emerald-400" />
              </div>
              <div>
                <h3 class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Pair Device</h3>
                <p class="text-[11px] text-gray-400 dark:text-gray-500">Connect another device to sync</p>
              </div>
            </div>
            <button @click="handleClose" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors cursor-pointer" aria-label="Handle Close">
              <X class="w-4 h-4" />
            </button>
          </div>

          <!-- Body -->
          <div class="p-5 min-h-[280px] flex flex-col">
            <!-- Step: Choose -->
            <div v-if="step === 'choose'" class="space-y-3 flex-1">
              <p class="text-[13px] text-gray-500 dark:text-gray-400 mb-4">Choose how to pair this device with another one running Synabit.</p>

              <!-- Generate Code Option -->
              <button
                @click="handleGenerate"
                class="w-full p-4 rounded-xl border-2 border-[#e6e6e6] dark:border-[#2c2c2c] hover:border-emerald-400 dark:hover:border-emerald-600 text-left transition-all flex items-start gap-3 cursor-pointer group"
              >
                <div class="w-10 h-10 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 flex items-center justify-center shrink-0 mt-0.5 group-hover:bg-emerald-100 dark:group-hover:bg-emerald-900/40 transition-colors">
                  <QrCode class="w-5 h-5 text-emerald-500" />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Generate a code</p>
                  <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Show a code on this device and enter it on the other one</p>
                </div>
              </button>

              <!-- Enter Code Option -->
              <button
                @click="handleEnterMode"
                class="w-full p-4 rounded-xl border-2 border-[#e6e6e6] dark:border-[#2c2c2c] hover:border-emerald-400 dark:hover:border-emerald-600 text-left transition-all flex items-start gap-3 cursor-pointer group"
              >
                <div class="w-10 h-10 rounded-lg bg-blue-50 dark:bg-blue-900/20 flex items-center justify-center shrink-0 mt-0.5 group-hover:bg-blue-100 dark:group-hover:bg-blue-900/40 transition-colors">
                  <Keyboard class="w-5 h-5 text-blue-500" />
                </div>
                <div class="flex-1 min-w-0">
                  <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Enter a code</p>
                  <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Enter the code shown on the other device</p>
                </div>
              </button>
            </div>

            <!-- Step: Generate Code -->
            <div v-else-if="step === 'generate'" class="flex-1 flex flex-col items-center justify-center">
              <!-- Loading -->
              <template v-if="isPairing && !pairingCode">
                <div class="flex flex-col items-center gap-4">
                  <Loader2 class="w-8 h-8 text-emerald-500 animate-spin" />
                  <p class="text-[13px] text-gray-500 dark:text-gray-400">Generating pairing code...</p>
                </div>
              </template>

              <!-- Code Display -->
              <template v-else-if="pairingCode">
                <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-5 text-center">Enter this code on the other device</p>
                
                <!-- Large Code Display -->
                <div class="relative mb-4">
                  <div class="flex items-center gap-3 px-6 py-4 bg-[#f8f8f8] dark:bg-[#1e1e1e] rounded-2xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <span class="text-3xl font-mono font-bold tracking-[0.25em] text-[#1c1c1e] dark:text-[#f4f4f5] select-all">
                      {{ pairingCode }}
                    </span>
                  </div>
                  <button
                    @click="handleCopyCode"
                    class="absolute -right-2 -top-2 p-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] shadow-sm hover:bg-gray-50 dark:hover:bg-[#333] transition-colors cursor-pointer"
                    :title="codeCopied ? 'Copied!' : 'Copy code'"
                  >
                    <Check v-if="codeCopied" class="w-3.5 h-3.5 text-emerald-500" />
                    <Copy v-else class="w-3.5 h-3.5 text-gray-400" />
                  </button>
                </div>

                <!-- Countdown -->
                <div v-if="countdown > 0" class="flex items-center gap-2 text-[12px] text-gray-400 dark:text-gray-500 mb-4">
                  <Clock class="w-3.5 h-3.5" />
                  <span>Expires in <span class="font-mono font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ countdownFormatted }}</span></span>
                </div>

                <!-- Waiting indicator -->
                <div class="flex items-center gap-2 text-[11px] text-emerald-600 dark:text-emerald-400">
                  <div class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></div>
                  <span>Waiting for other device...</span>
                </div>
              </template>

              <!-- Success -->
              <template v-if="pairingSuccess">
                <div class="flex flex-col items-center gap-3 text-center">
                  <div class="w-14 h-14 rounded-full bg-emerald-100 dark:bg-emerald-900/30 flex items-center justify-center">
                    <Check class="w-7 h-7 text-emerald-600 dark:text-emerald-400" />
                  </div>
                  <p class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Device paired!</p>
                  <p class="text-[12px] text-gray-400 dark:text-gray-500">Your devices will now sync automatically.</p>
                </div>
              </template>

              <!-- Error -->
              <div v-if="error" class="mt-4 w-full">
                <div class="flex items-start gap-2 px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-[12px] text-red-600 dark:text-red-400">
                  <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
                  <span>{{ error }}</span>
                </div>
              </div>
            </div>

            <!-- Step: Enter Code -->
            <div v-else-if="step === 'enter'" class="flex-1 flex flex-col items-center justify-center">
              <template v-if="!pairingSuccess">
                <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-6 text-center">Enter the 8-character code shown on the other device</p>

                <!-- Code Input Grid -->
                <div class="flex items-center gap-1 mb-6">
                  <template v-for="(_, index) in joinCodeParts" :key="index">
                    <!-- Dash separator after 4th character -->
                    <div v-if="index === 4" class="text-xl font-bold text-gray-300 dark:text-gray-600 mx-1">-</div>
                    <input
                      :ref="(el) => { if (el) joinInputRefs[index] = el as HTMLInputElement }"
                      :value="joinCodeParts[index]"
                      @input="handleCodeInput(index, $event)"
                      @keydown="handleCodeKeydown(index, $event)"
                      maxlength="1"
                      class="w-10 h-12 text-center text-lg font-mono font-bold rounded-lg bg-[#f8f8f8] dark:bg-[#1e1e1e] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-2 focus:ring-emerald-500/30 focus:border-emerald-500 transition-all uppercase"
                    />
                  </template>
                </div>

                <!-- Join button -->
                <button
                  @click="handleJoin"
                  :disabled="isJoining || fullJoinCode().replace('-', '').length !== 8"
                  class="px-6 py-2.5 bg-emerald-600 hover:bg-emerald-700 text-white rounded-xl text-[13px] font-medium transition-all shadow-sm flex items-center justify-center gap-2 disabled:opacity-60 cursor-pointer disabled:cursor-not-allowed"
                >
                  <Loader2 v-if="isJoining" class="w-4 h-4 animate-spin" />
                  <Check v-else class="w-4 h-4" />
                  {{ isJoining ? 'Pairing...' : 'Pair Device' }}
                </button>
              </template>

              <!-- Success -->
              <template v-if="pairingSuccess">
                <div class="flex flex-col items-center gap-3 text-center">
                  <div class="w-14 h-14 rounded-full bg-emerald-100 dark:bg-emerald-900/30 flex items-center justify-center">
                    <Check class="w-7 h-7 text-emerald-600 dark:text-emerald-400" />
                  </div>
                  <p class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Device paired!</p>
                  <p class="text-[12px] text-gray-400 dark:text-gray-500">Your devices will now sync automatically.</p>
                </div>
              </template>

              <!-- Error -->
              <div v-if="error" class="mt-4 w-full">
                <div class="flex items-start gap-2 px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-[12px] text-red-600 dark:text-red-400">
                  <AlertCircle class="w-4 h-4 shrink-0 mt-0.5" />
                  <span>{{ error }}</span>
                </div>
              </div>
            </div>
          </div>

          <!-- Footer -->
          <div class="p-4 bg-[#f8f8f8]/50 dark:bg-[#1e1e1e]/50 flex justify-between border-t border-[#e6e6e6] dark:border-[#2c2c2c]">
            <button
              v-if="step !== 'choose'"
              @click="handleBack"
              class="px-4 py-2 text-[12px] font-medium rounded-lg text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] transition-colors cursor-pointer"
            >
              ← Back
            </button>
            <div v-else></div>
            <button
              @click="handleClose"
              class="px-4 py-2 text-[12px] font-medium rounded-lg text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] transition-colors cursor-pointer"
            >
              {{ pairingSuccess ? 'Done' : 'Cancel' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.pairing-modal-enter-active,
.pairing-modal-leave-active {
  transition: opacity 0.2s ease;
}
.pairing-modal-enter-active > div:last-child,
.pairing-modal-leave-active > div:last-child {
  transition: transform 0.2s ease, opacity 0.2s ease;
}
.pairing-modal-enter-from,
.pairing-modal-leave-to {
  opacity: 0;
}
.pairing-modal-enter-from > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}
.pairing-modal-leave-to > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}

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
