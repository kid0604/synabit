<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { Shield, Delete } from 'lucide-vue-next';
import { useAppLockStore } from '../../stores/useAppLockStore';

const props = withDefaults(defineProps<{
  title: string;
  cancellable?: boolean;
}>(), {
  cancellable: true,
});

const emit = defineEmits<{
  (e: 'unlocked'): void;
  (e: 'cancelled'): void;
}>();

const store = useAppLockStore();

// PIN state
const pin = ref<string[]>([]);
const maxLength = 6;
const isVerifying = ref(false);
const errorMessage = ref('');
const remainingAttempts = ref<number | null>(null);
const isShaking = ref(false);
const lockedUntil = ref<number | null>(null);
const lockoutCountdown = ref(0);
let countdownInterval: ReturnType<typeof setInterval> | null = null;

const isLockedOut = computed(() => lockoutCountdown.value > 0);

const formattedCountdown = computed(() => {
  const secs = lockoutCountdown.value;
  const mins = Math.floor(secs / 60);
  const remaining = secs % 60;
  return mins > 0
    ? `${mins}:${remaining.toString().padStart(2, '0')}`
    : `${remaining}s`;
});

function startLockoutCountdown(until: number) {
  lockedUntil.value = until;
  updateCountdown();
  if (countdownInterval) clearInterval(countdownInterval);
  countdownInterval = setInterval(() => {
    updateCountdown();
    if (lockoutCountdown.value <= 0 && countdownInterval) {
      clearInterval(countdownInterval);
      countdownInterval = null;
    }
  }, 1000);
}

function updateCountdown() {
  if (!lockedUntil.value) {
    lockoutCountdown.value = 0;
    return;
  }
  const now = Date.now() / 1000;
  lockoutCountdown.value = Math.max(0, Math.ceil(lockedUntil.value - now));
}

function addDigit(digit: string) {
  if (isLockedOut.value || isVerifying.value) return;
  if (pin.value.length >= maxLength) return;
  pin.value.push(digit);

  // Auto-submit when 6 digits entered
  if (pin.value.length === maxLength) {
    verify();
  }
}

function removeDigit() {
  if (isLockedOut.value || isVerifying.value) return;
  pin.value.pop();
  errorMessage.value = '';
}

async function verify() {
  if (isVerifying.value) return;
  isVerifying.value = true;
  errorMessage.value = '';

  try {
    const result = await store.verifyPin(pin.value.join(''));
    if (result.success) {
      emit('unlocked');
    } else {
      remainingAttempts.value = result.remaining_attempts;

      if (result.locked_until) {
        startLockoutCountdown(result.locked_until);
        errorMessage.value = 'Too many attempts. Please wait.';
      } else {
        errorMessage.value = `Wrong PIN. ${result.remaining_attempts} attempt${result.remaining_attempts !== 1 ? 's' : ''} remaining.`;
      }

      // Shake animation
      isShaking.value = true;
      setTimeout(() => {
        isShaking.value = false;
      }, 500);

      pin.value = [];
    }
  } catch (e) {
    errorMessage.value = 'Verification failed. Try again.';
    pin.value = [];
  } finally {
    isVerifying.value = false;
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key >= '0' && e.key <= '9') {
    addDigit(e.key);
  } else if (e.key === 'Backspace') {
    removeDigit();
  } else if (e.key === 'Escape' && props.cancellable) {
    emit('cancelled');
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKeyDown);
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyDown);
  if (countdownInterval) {
    clearInterval(countdownInterval);
  }
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
    <Transition name="lockscreen">
      <div class="fixed inset-0 z-[9999] flex items-center justify-center select-none lock-backdrop">
        <!-- Gradient background layer -->
        <div class="absolute inset-0 bg-gradient-to-br from-[#fdfdfc] via-[#f0eff5] to-[#e8e6f0] dark:from-[#1a1a2e] dark:via-[#16162a] dark:to-[#0f0f1a]"></div>

        <!-- Subtle pattern overlay -->
        <div class="absolute inset-0 opacity-[0.03] dark:opacity-[0.05]"
          style="background-image: radial-gradient(circle at 1px 1px, currentColor 1px, transparent 0); background-size: 40px 40px;">
        </div>

        <!-- Frosted glass content card -->
        <div class="relative z-10 flex flex-col items-center w-full max-w-[340px] px-8 py-10">

          <!-- App Icon -->
          <div class="w-20 h-20 rounded-[22px] bg-gradient-to-br from-[#7c3aed] to-[#a78bfa] dark:from-[#a78bfa] dark:to-[#7c3aed] flex items-center justify-center shadow-lg shadow-purple-500/20 dark:shadow-purple-400/10 mb-6">
            <Shield class="w-10 h-10 text-white" :stroke-width="1.5" />
          </div>

          <!-- Title -->
          <h1 class="text-[17px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-1 text-center">
            {{ title }}
          </h1>
          <p class="text-[13px] text-[#8b8b8b] dark:text-[#71717a] mb-8 text-center">
            Enter your 6-digit PIN
          </p>

          <!-- PIN Dots -->
          <div
            class="flex items-center gap-3.5 mb-6"
            :class="{ 'shake': isShaking }"
          >
            <div
              v-for="i in maxLength"
              :key="i"
              class="w-3.5 h-3.5 rounded-full border-2 transition-all duration-200 ease-out"
              :class="[
                i <= pin.length
                  ? 'bg-[#7c3aed] dark:bg-[#a78bfa] border-[#7c3aed] dark:border-[#a78bfa] scale-110'
                  : 'bg-transparent border-[#d4d4d8] dark:border-[#3f3f46]',
                errorMessage && pin.length === 0 ? 'border-red-400 dark:border-red-500' : ''
              ]"
            ></div>
          </div>

          <!-- Error Message -->
          <Transition name="fade">
            <p v-if="errorMessage && !isLockedOut" class="text-[12px] text-red-500 dark:text-red-400 font-medium mb-4 text-center min-h-[18px]">
              {{ errorMessage }}
            </p>
          </Transition>

          <!-- Lockout Countdown -->
          <Transition name="fade">
            <div v-if="isLockedOut" class="flex flex-col items-center mb-4">
              <div class="px-4 py-2 rounded-xl bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/40">
                <p class="text-[12px] text-red-600 dark:text-red-400 font-semibold text-center">
                  Locked out · Try again in {{ formattedCountdown }}
                </p>
              </div>
            </div>
          </Transition>

          <!-- Number Pad -->
          <div class="grid grid-cols-3 gap-3 w-full max-w-[260px]">
            <template v-for="(row, ri) in numPadKeys" :key="ri">
              <template v-for="key in row" :key="key">
                <!-- Backspace -->
                <button
                  v-if="key === 'back'"
                  @click="removeDigit"
                  :disabled="isLockedOut || isVerifying || pin.length === 0"
                  class="numpad-btn numpad-action"
                 aria-label="Remove Digit">
                  <Delete class="w-5 h-5" />
                </button>

                <!-- Empty spacer -->
                <div v-else-if="key === ''" class="w-full"></div>

                <!-- Digit buttons -->
                <button
                  v-else
                  @click="addDigit(key)"
                  :disabled="isLockedOut || isVerifying"
                  class="numpad-btn numpad-digit"
                >
                  {{ key }}
                </button>
              </template>
            </template>
          </div>

          <!-- Verifying indicator -->
          <Transition name="fade">
            <div v-if="isVerifying" class="mt-6 flex items-center gap-2">
              <div class="w-4 h-4 border-2 border-[#7c3aed] dark:border-[#a78bfa] border-t-transparent rounded-full animate-spin"></div>
              <span class="text-[12px] text-[#8b8b8b] dark:text-[#71717a]">Verifying…</span>
            </div>
          </Transition>

          <!-- Cancel button -->
          <button
            v-if="props.cancellable"
            @click="emit('cancelled')"
            class="mt-6 px-6 py-2 text-[13px] text-[#8b8b8b] dark:text-[#71717a] hover:text-[#1c1c1e] dark:hover:text-[#f4f4f5] hover:bg-black/5 dark:hover:bg-white/5 rounded-lg transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* Number pad buttons */
.numpad-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  aspect-ratio: 1;
  border-radius: 50%;
  font-size: 22px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s ease;
  border: none;
  outline: none;
  -webkit-tap-highlight-color: transparent;
  user-select: none;
}

.numpad-digit {
  background: rgba(255, 255, 255, 0.7);
  color: #1c1c1e;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06), 0 0 0 1px rgba(0, 0, 0, 0.04);
}

:is(.dark) .numpad-digit {
  background: rgba(255, 255, 255, 0.08);
  color: #f4f4f5;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2), 0 0 0 1px rgba(255, 255, 255, 0.06);
}

.numpad-digit:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.95);
  transform: scale(1.05);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08), 0 0 0 1px rgba(0, 0, 0, 0.06);
}

:is(.dark) .numpad-digit:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.14);
}

.numpad-digit:active:not(:disabled) {
  transform: scale(0.95);
  background: rgba(0, 0, 0, 0.05);
}

:is(.dark) .numpad-digit:active:not(:disabled) {
  background: rgba(255, 255, 255, 0.2);
}

.numpad-action {
  background: transparent;
  color: #52525b;
  font-size: 18px;
}

:is(.dark) .numpad-action {
  color: #a1a1aa;
}

.numpad-action:hover:not(:disabled) {
  background: rgba(0, 0, 0, 0.04);
  color: #1c1c1e;
}

:is(.dark) .numpad-action:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.06);
  color: #f4f4f5;
}

.numpad-action:active:not(:disabled) {
  transform: scale(0.9);
}

.numpad-btn:disabled {
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

/* Lockscreen transition */
.lockscreen-enter-active {
  transition: opacity 0.3s ease;
}
.lockscreen-leave-active {
  transition: opacity 0.25s ease;
}
.lockscreen-enter-from,
.lockscreen-leave-to {
  opacity: 0;
}

/* Fade for error/lockout */
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
