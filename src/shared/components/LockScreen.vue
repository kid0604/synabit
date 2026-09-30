<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';
import { Shield, Delete, Copy } from 'lucide-vue-next';
import { useAppLockStore, pinErrorKey, type ResetChallenge } from '../../stores/useAppLockStore';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

const props = withDefaults(defineProps<{
  title: string;
  cancellable?: boolean;
}>(), {
  cancellable: true,
});

const emit = defineEmits<{
  /**
   * The PIN checked out. It comes along for a caller whose action the backend
   * guards as well (`remove_app_lock`, `set_family_safe`) and wants it again;
   * callers that only needed the check can ignore it.
   */
  (e: 'unlocked', pin: string): void;
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
    const entered = pin.value.join('');
    const result = await store.verifyPin(entered);
    if (result.success) {
      emit('unlocked', entered);
    } else {
      remainingAttempts.value = result.remaining_attempts;

      if (result.locked_until) {
        startLockoutCountdown(result.locked_until);
        errorMessage.value = t('shell.lock.too_many');
      } else {
        errorMessage.value = t('shell.lock.wrong_pin', { count: result.remaining_attempts }, result.remaining_attempts);
      }

      // Shake animation
      isShaking.value = true;
      setTimeout(() => {
        isShaking.value = false;
      }, 500);

      pin.value = [];
    }
  } catch {
    errorMessage.value = t('shell.lock.verify_failed');
    pin.value = [];
  } finally {
    isVerifying.value = false;
  }
}

// ── Forgot PIN ──
//
// The PIN locks the app window, not the files, so forgetting it loses
// nothing but the way in. A reset that asked only the app could be used by
// anybody at the app — including whoever a family-safe PIN was set for — so
// the backend asks for proof of reaching this computer's app-data folder
// through the operating system instead: a folder with a one-time name, made
// by hand. See `src-tauri/src/commands/app_lock.rs`.
const showForgot = ref(false);
const challenge = ref<ResetChallenge | null>(null);
const resetUnavailable = ref(false);
const resetError = ref('');
const isResetting = ref(false);

async function openForgot() {
  showForgot.value = true;
  resetError.value = '';
  try {
    challenge.value = await store.beginReset();
  } catch (e) {
    // A phone, where the folder cannot be reached; or the folder could not be
    // made. Either way the explanation still stands on its own.
    challenge.value = null;
    resetUnavailable.value = true;
    if (!pinErrorKey(e)) resetError.value = t('shell.lock.reset_failed');
  }
}

async function copyFolder() {
  if (!challenge.value) return;
  try {
    await navigator.clipboard.writeText(challenge.value.folder);
  } catch {
    // The path is on screen and selectable; copying is a convenience.
  }
}

async function finishReset() {
  if (isResetting.value) return;
  isResetting.value = true;
  resetError.value = '';
  try {
    await store.finishReset();
    // Every lock screen shown because a PIN is set is gone with it. One shown
    // for a pending action (a settings change) is closed without doing that
    // action: it can be done again now, deliberately. Vue drops the event if
    // this screen was already removed.
    await nextTick();
    emit('cancelled');
  } catch (e) {
    const key = pinErrorKey(e);
    resetError.value = t(key ?? 'shell.lock.reset_failed');
  } finally {
    isResetting.value = false;
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (showForgot.value) {
    if (e.key === 'Escape') showForgot.value = false;
    return;
  }
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
    <Transition appear name="lockscreen">
      <!-- Above everything, raised dialogs (10000) included: a lock that a
           dialog can sit on top of is not a lock. -->
      <div v-show="true" class="fixed inset-0 z-[20000] flex items-center justify-center select-none lock-backdrop">
        <!-- Background layer -->
        <div class="absolute inset-0 bg-base dark:bg-base-dark"></div>

        <!-- Subtle pattern overlay -->
        <div class="absolute inset-0 opacity-[0.03] dark:opacity-[0.05]"
          style="background-image: radial-gradient(circle at 1px 1px, currentColor 1px, transparent 0); background-size: 40px 40px;">
        </div>

        <!-- Frosted glass content card -->
        <div class="relative z-10 flex flex-col items-center w-full max-w-[340px] px-8 py-10">

          <!-- App Icon -->
          <div class="w-20 h-20 rounded-[22px] bg-accent flex items-center justify-center shadow-lg shadow-accent/20 mb-6">
            <Shield class="w-10 h-10 text-white" :stroke-width="1.5" />
          </div>

          <!-- Title -->
          <h1 class="text-[17px] font-semibold text-text dark:text-text-dark mb-1 text-center">
            {{ title }}
          </h1>
          <p class="text-[13px] text-muted dark:text-muted-dark mb-8 text-center">
            {{ $t('shell.lock.enter_pin') }}
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
                  ? 'bg-accent dark:bg-accent-dark border-accent dark:border-accent-dark scale-110'
                  : 'bg-transparent border-[#d4d4d8] dark:border-[#3f3f46]',
                errorMessage && pin.length === 0 ? 'border-red-400 dark:border-red-500' : ''
              ]"
            ></div>
          </div>

          <!-- Error Message -->
          <Transition name="fade">
            <p v-if="errorMessage && !isLockedOut" role="alert" class="text-[12px] text-red-500 dark:text-red-400 font-medium mb-4 text-center min-h-[18px]">
              {{ errorMessage }}
            </p>
          </Transition>

          <!-- Lockout Countdown -->
          <Transition name="fade">
            <div v-if="isLockedOut" class="flex flex-col items-center mb-4">
              <div class="px-4 py-2 rounded-xl bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/40">
                <p class="text-[12px] text-red-600 dark:text-red-400 font-semibold text-center">
                  {{ $t('shell.lock.locked_out', { time: formattedCountdown }) }}
                </p>
              </div>
            </div>
          </Transition>

          <!-- Number Pad -->
          <div v-if="!showForgot" class="grid grid-cols-3 gap-3 w-full max-w-[260px]">
            <template v-for="(row, ri) in numPadKeys" :key="ri">
              <template v-for="key in row" :key="key">
                <!-- Backspace -->
                <button
                  v-if="key === 'back'"
                  @click="removeDigit"
                  :disabled="isLockedOut || isVerifying || pin.length === 0"
                  class="numpad-btn numpad-action"
                 :aria-label="$t('shell.lock.remove_digit')">
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
              <div class="w-4 h-4 border-2 border-accent dark:border-accent-dark border-t-transparent rounded-full animate-spin"></div>
              <span class="text-[12px] text-muted dark:text-muted-dark">{{ $t('shell.lock.verifying') }}</span>
            </div>
          </Transition>

          <!-- Forgot PIN: what the PIN does and does not guard, and the reset. -->
          <button
            v-if="!showForgot"
            type="button"
            class="mt-6 text-[13px] text-accent dark:text-accent-dark hover:underline"
            @click="openForgot"
          >
            {{ $t('shell.lock.forgot') }}
          </button>
          <div v-else class="w-full max-w-[300px] text-left select-text">
            <p class="text-[13px] text-text dark:text-text-dark leading-relaxed mb-3">
              {{ $t('shell.lock.forgot_hint') }}
            </p>
            <template v-if="challenge">
              <p class="text-xs text-[#6b6b6b] dark:text-[#8b8b93] leading-relaxed mb-1">
                {{ $t('shell.lock.reset_steps') }}
              </p>
              <code data-reset-name class="block break-all text-xs px-2 py-1.5 mb-2 rounded-lg bg-black/5 dark:bg-white/5 text-text dark:text-text-dark">{{ challenge.name }}</code>
              <p class="text-xs text-[#6b6b6b] dark:text-[#8b8b93] leading-relaxed mb-1">
                {{ $t('shell.lock.reset_where') }}
              </p>
              <div class="flex items-start gap-2 mb-2">
                <code data-reset-folder class="flex-1 min-w-0 break-all text-xs px-2 py-1.5 rounded-lg bg-black/5 dark:bg-white/5 text-text dark:text-text-dark">{{ challenge.folder }}</code>
                <button
                  type="button"
                  class="btn-icon shrink-0"
                  :aria-label="$t('shell.lock.copy_folder')"
                  :title="$t('shell.lock.copy_folder')"
                  @click="copyFolder"
                >
                  <Copy class="w-4 h-4" />
                </button>
              </div>
              <p class="text-xs text-[#6b6b6b] dark:text-[#8b8b93] leading-relaxed mb-3">
                {{ $t('shell.lock.reset_warning') }}
              </p>
            </template>
            <p v-else-if="resetUnavailable" class="text-xs text-[#6b6b6b] dark:text-[#8b8b93] leading-relaxed mb-3">
              {{ $t('shell.lock.reset_not_here') }}
            </p>
            <p v-if="resetError" role="alert" class="text-[12px] text-red-500 dark:text-red-400 font-medium mb-3">
              {{ resetError }}
            </p>
            <div class="flex gap-2 justify-end">
              <button type="button" class="btn-secondary" @click="showForgot = false">
                {{ $t('shell.common.back') }}
              </button>
              <button
                v-if="challenge"
                type="button"
                class="btn-primary"
                :disabled="isResetting"
                @click="finishReset"
              >
                {{ $t('shell.lock.reset_pin') }}
              </button>
            </div>
          </div>

          <!-- Cancel button -->
          <button
            v-if="props.cancellable"
            @click="emit('cancelled')"
            class="mt-6 px-6 py-2 text-[13px] text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark hover:bg-black/5 dark:hover:bg-white/5 rounded-lg transition-colors"
          >
            {{ $t('shell.common.cancel') }}
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
