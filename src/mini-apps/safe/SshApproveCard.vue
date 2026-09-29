<script setup lang="ts">
/**
 * "May `ssh` sign with this key?" — raised by Safe's SSH agent for each
 * signature while "ask each time" is on. It names the key and, for a login,
 * the user it logs in as; no answer within a minute is a no.
 */
import { onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Terminal } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';

const { t } = useI18n();

interface Ask {
  id: string;
  request: { key: string; fingerprint: string; login_as: string | null };
}

const queue = ref<Ask[]>([]);

async function answer(allow: boolean) {
  const ask = queue.value.shift();
  if (ask) await invoke('safe_ssh_answer', { id: ask.id, allow }).catch(() => undefined);
}

let unlisten: UnlistenFn | null = null;
onMounted(async () => {
  unlisten = await listen<Ask>('safe://ssh-approve', (e) => queue.value.push(e.payload));
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <ModalDialog
    v-if="queue.length"
    :show="true"
    labelled-by="ssh-approve-title"
    card-class="max-w-[420px] text-text dark:text-text-dark"
    @close="answer(false)"
  >
    <div class="p-5 space-y-4">
      <div class="flex items-start gap-3">
        <div class="w-9 h-9 rounded-xl bg-accent/10 flex items-center justify-center flex-shrink-0">
          <Terminal class="w-4 h-4 text-accent" />
        </div>
        <div class="space-y-1 min-w-0">
          <h2 id="ssh-approve-title" class="font-semibold">{{ t('safe.ssh.approve_title') }}</h2>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">
            {{ queue[0].request.login_as ? t('safe.ssh.approve_login', { key: queue[0].request.key, user: queue[0].request.login_as }) : t('safe.ssh.approve_sign', { key: queue[0].request.key }) }}
          </p>
          <p class="text-xs font-mono text-text-tertiary dark:text-text-tertiary-dark truncate">{{ queue[0].request.fingerprint }}</p>
        </div>
      </div>
      <div class="flex justify-end gap-2">
        <button class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="answer(false)">{{ t('safe.ssh.deny') }}</button>
        <button class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium" autofocus @click="answer(true)">{{ t('safe.ssh.allow') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
