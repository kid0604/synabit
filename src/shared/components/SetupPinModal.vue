<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { Shield, Delete, X, Check } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { useAppLockStore } from '../../stores/useAppLockStore';

const props = defineProps<{
  mode: 'setup' | 'change';
}>();

const emit = defineEmits<{
  (e: 'done'): void;
  (e: 'cancel'): void;
}>();

const store = useAppLockStore();

// Steps: 'change' mode has step 0 (verify old), both have step 1 (new) and step 2 (confirm)
const currentStep = ref(props.mode === 'change' ? 0 : 1);
const pin = ref<string[]>([]);
const maxLength = 6;
const isProcessing = ref(false);
const errorMessage = ref('');
const isShaking = ref(false);

// Stored PINs across steps
const oldPin = ref('');
const newPin = ref('');

const totalSteps = computed(() => (props.mode === 'change' ? 3 : 2));

const stepTitle = computed(() => {
  if (props.mode === 'change') {
    if (currentStep.value === 0) return 'Enter Current PIN';
    if (currentStep.value === 1) return 'Create New PIN';
    return 'Confirm New PIN';
  }
  if (currentStep.value === 1) return 'Create a 6-digit PIN';
  return 'Confirm Your PIN';
});

const stepDescription = computed(() => {
  if (props.mode === 'change') {
    if (currentStep.value === 0) return 'Verify your identity first';
    if (currentStep.value === 1) return 'Choose a new 6-digit PIN';
    return 'Re-enter your new PIN to confirm';
  }
  if (currentStep.value === 1) return 'This PIN will protect your app';
  return 'Re-enter the same PIN to confirm';
});

// Step indicators
const steps = computed(() => {
  if (props.mode === 'change') {
    return [
      { num: 0, label: 'Verify' },
      { num: 1, label: 'New PIN' },
      { num: 2, label: 'Confirm' },
    ];
  }
  return [
    { num: 1, label: 'Create' },
    { num: 2, label: 'Confirm' },
  ];
});

function addDigit(digit: string) {
  if (isProcessing.value) return;
  if (pin.value.length >= maxLength) return;
  pin.value.push(digit);

  if (pin.value.length === maxLength) {
    handleStepComplete();
  }
}

function removeDigit() {
  if (isProcessing.value) return;
  pin.value.pop();
  errorMessage.value = '';
}

function triggerShake() {
  isShaking.value = true;
  setTimeout(() => { isShaking.value = false; }, 500);
}

async function handleStepComplete() {
  const enteredPin = pin.value.join('');
  isProcessing.value = true;
  errorMessage.value = '';

  try {
    if (props.mode === 'change' && currentStep.value === 0) {
      // Verify old PIN
      const result = await store.verifyPin(enteredPin);
      if (!result.success) {
        errorMessage.value = result.locked_until
          ? 'Too many attempts. Please wait.'
          : `Wrong PIN. ${result.remaining_attempts} attempt${result.remaining_attempts !== 1 ? 's' : ''} left.`;
        triggerShake();
        pin.value = [];
        return;
      }
      oldPin.value = enteredPin;
      pin.value = [];
      currentStep.value = 1;
    } else if (currentStep.value === 1) {
      // Store new PIN, move to confirm
      newPin.value = enteredPin;
      pin.value = [];
      currentStep.value = 2;
    } else if (currentStep.value === 2) {
      // Confirm PIN
      if (enteredPin !== newPin.value) {
        errorMessage.value = "PINs don't match. Try again.";
        triggerShake();
        pin.value = [];
        newPin.value = '';
        currentStep.value = 1;
        return;
      }

      // Call backend
      if (props.mode === 'setup') {
        await invoke('setup_app_lock', { pin: newPin.value });
      } else {
        await invoke('change_app_lock', { oldPin: oldPin.value, newPin: newPin.value });
      }

      // Refresh store config
      await store.refreshConfig();
      emit('done');
    }
  } catch (e) {
    errorMessage.value = String(e);
    triggerShake();
    pin.value = [];
  } finally {
    isProcessing.value = false;
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key >= '0' && e.key <= '9') {
    addDigit(e.key);
  } else if (e.key === 'Backspace') {
    removeDigit();
  } else if (e.key === 'Escape') {
    emit('cancel');
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeyDown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyDown);
});

const numPadKeys = [
  ['1', '2', '3'],
  ['4', '5', '6'],
  ['7', '8', '9'],
  ['back', '0', ''],
];
</script>

<template>
  <Teleport to="body">
    <Transition name="pin-modal">
      <div class="fixed inset-0 z-[9998] flex items-center justify-center">
        <!-- Backdrop -->
        <div
          class="absolute inset-0 bg-black/40 dark:bg-black/60 backdrop-blur-sm"
          @mousedown="emit('cancel')"
        ></div>

        <!-- Modal Card -->
        <div
          class="relative z-10 w-[95vw] max-w-[380px] bg-[#fdfdfc] dark:bg-[#242424] rounded-2xl shadow-2xl border border-[#e6e6e6] dark:border-[#333] overflow-hidden"
          @mousedown.stop
        >
          <!-- Header -->
          <div class="flex items-center justify-between px-5 pt-5 pb-0">
            <div class="flex items-center gap-2">
              <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-[#7c3aed] to-[#a78bfa] dark:from-[#a78bfa] dark:to-[#7c3aed] flex items-center justify-center">
                <Shield class="w-4 h-4 text-white" />
              </div>
              <span class="text-[14px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
                {{ mode === 'setup' ? 'Set Up PIN' : 'Change PIN' }}
              </span>
            </div>
            <button
              @click="emit('cancel')"
              class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors"
            >
              <X class="w-4 h-4" />
            </button>
          </div>

          <!-- Step Indicator -->
          <div class="flex items-center justify-center gap-2 px-5 pt-4 pb-2">
            <template v-for="(step, idx) in steps" :key="step.num">
              <div
                class="flex items-center gap-1.5"
              >
                <div
                  class="w-6 h-6 rounded-full flex items-center justify-center text-[11px] font-bold transition-all duration-300"
                  :class="[
                    currentStep > step.num
                      ? 'bg-green-500 text-white'
                      : currentStep === step.num
                        ? 'bg-[#7c3aed] dark:bg-[#a78bfa] text-white'
                        : 'bg-[#e6e6e6] dark:bg-[#3a3a3a] text-[#8b8b8b] dark:text-[#71717a]'
                  ]"
                >
                  <Check v-if="currentStep > step.num" class="w-3.5 h-3.5" />
                  <span v-else>{{ idx + 1 }}</span>
                </div>
                <span
                  class="text-[11px] font-medium transition-colors"
                  :class="currentStep === step.num ? 'text-[#1c1c1e] dark:text-[#f4f4f5]' : 'text-[#8b8b8b] dark:text-[#71717a]'"
                >
                  {{ step.label }}
                </span>
              </div>
              <div
                v-if="idx < steps.length - 1"
                class="w-6 h-[1.5px] rounded transition-colors"
                :class="currentStep > step.num ? 'bg-green-500' : 'bg-[#e6e6e6] dark:bg-[#3a3a3a]'"
              ></div>
            </template>
          </div>

          <!-- Content -->
          <div class="flex flex-col items-center px-5 pb-6 pt-3">
            <!-- Step Title -->
            <h3 class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-0.5 text-center">
              {{ stepTitle }}
            </h3>
            <p class="text-[12px] text-[#8b8b8b] dark:text-[#71717a] mb-5 text-center">
              {{ stepDescription }}
            </p>

            <!-- PIN Dots -->
            <div
              class="flex items-center gap-3 mb-4"
              :class="{ 'shake': isShaking }"
            >
              <div
                v-for="i in maxLength"
                :key="i"
                class="w-3 h-3 rounded-full border-2 transition-all duration-200 ease-out"
                :class="[
                  i <= pin.length
                    ? 'bg-[#7c3aed] dark:bg-[#a78bfa] border-[#7c3aed] dark:border-[#a78bfa] scale-110'
                    : 'bg-transparent border-[#d4d4d8] dark:border-[#3f3f46]',
                ]"
              ></div>
            </div>

            <!-- Error Message -->
            <Transition name="fade">
              <p
                v-if="errorMessage"
                class="text-[11px] text-red-500 dark:text-red-400 font-medium mb-3 text-center min-h-[16px]"
              >
                {{ errorMessage }}
              </p>
            </Transition>

            <!-- Number Pad -->
            <div class="grid grid-cols-3 gap-2.5 w-full max-w-[230px]">
              <template v-for="(row, ri) in numPadKeys" :key="ri">
                <template v-for="key in row" :key="key">
                  <!-- Backspace -->
                  <button
                    v-if="key === 'back'"
                    @click="removeDigit"
                    :disabled="isProcessing || pin.length === 0"
                    class="modal-numpad-btn modal-numpad-action"
                  >
                    <Delete class="w-4.5 h-4.5" />
                  </button>

                  <!-- Empty spacer -->
                  <div v-else-if="key === ''" class="w-full"></div>

                  <!-- Digit buttons -->
                  <button
                    v-else
                    @click="addDigit(key)"
                    :disabled="isProcessing"
                    class="modal-numpad-btn modal-numpad-digit"
                  >
                    {{ key }}
                  </button>
                </template>
              </template>
            </div>

            <!-- Processing indicator -->
            <Transition name="fade">
              <div v-if="isProcessing" class="mt-4 flex items-center gap-2">
                <div class="w-3.5 h-3.5 border-2 border-[#7c3aed] dark:border-[#a78bfa] border-t-transparent rounded-full animate-spin"></div>
                <span class="text-[11px] text-[#8b8b8b] dark:text-[#71717a]">Processing…</span>
              </div>
            </Transition>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* Number pad buttons (modal variant — slightly smaller) */
.modal-numpad-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  aspect-ratio: 1;
  border-radius: 50%;
  font-size: 20px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  border: none;
  outline: none;
  -webkit-tap-highlight-color: transparent;
  user-select: none;
}

.modal-numpad-digit {
  background: rgba(0, 0, 0, 0.03);
  color: #1c1c1e;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04);
}

:is(.dark) .modal-numpad-digit {
  background: rgba(255, 255, 255, 0.07);
  color: #f4f4f5;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
}

.modal-numpad-digit:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.07);
  transform: scale(1.05);
}

:is(.dark) .modal-numpad-digit:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.12);
}

.modal-numpad-digit:active:not(:disabled) {
  transform: scale(0.95);
  background: rgba(0, 0, 0, 0.1);
}

:is(.dark) .modal-numpad-digit:active:not(:disabled) {
  background: rgba(255, 255, 255, 0.18);
}

.modal-numpad-action {
  background: transparent;
  color: #52525b;
  font-size: 16px;
}

:is(.dark) .modal-numpad-action {
  color: #a1a1aa;
}

.modal-numpad-action:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
  color: #1c1c1e;
}

:is(.dark) .modal-numpad-action:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
  color: #f4f4f5;
}

.modal-numpad-action:active:not(:disabled) {
  transform: scale(0.9);
}

.modal-numpad-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

/* Shake animation */
@keyframes shake {
  0%, 100% { transform: translateX(0); }
  10%, 30%, 50%, 70%, 90% { transform: translateX(-6px); }
  20%, 40%, 60%, 80% { transform: translateX(6px); }
}

.shake {
  animation: shake 0.5s cubic-bezier(0.36, 0.07, 0.19, 0.97);
}

/* Modal transition */
.pin-modal-enter-active {
  transition: opacity 0.2s ease;
}
.pin-modal-enter-active > div:last-child {
  transition: transform 0.2s ease, opacity 0.2s ease;
}
.pin-modal-leave-active {
  transition: opacity 0.15s ease;
}
.pin-modal-leave-active > div:last-child {
  transition: transform 0.15s ease, opacity 0.15s ease;
}
.pin-modal-enter-from,
.pin-modal-leave-to {
  opacity: 0;
}
.pin-modal-enter-from > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}
.pin-modal-leave-to > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}

/* Fade */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease, transform 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
