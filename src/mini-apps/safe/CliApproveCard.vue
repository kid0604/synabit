<script setup lang="ts">
/**
 * "May this command have these secrets?" — raised by `synabit-safe run`. It
 * names the command, the folder it runs in and every variable with the item it
 * comes from; no answer within a minute is a no.
 */
import { onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { SquareTerminal } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';

const { t } = useI18n();

interface Ask {
  id: string;
  command: string;
  cwd: string;
  secrets: string[];
}

const queue = ref<Ask[]>([]);

async function answer(allow: boolean) {
  const ask = queue.value.shift();
  if (ask) await invoke('safe_ssh_answer', { id: ask.id, allow }).catch(() => undefined);
}

let unlisten: UnlistenFn | null = null;
onMounted(async () => {
  unlisten = await listen<Ask>('safe://cli-approve', (e) => queue.value.push(e.payload));
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <ModalDialog
    v-if="queue.length"
    :show="true"
    labelled-by="cli-approve-title"
    card-class="max-w-[460px] text-text dark:text-text-dark"
    @close="answer(false)"
  >
    <div class="p-5 space-y-4">
      <div class="flex items-start gap-3">
        <div class="w-9 h-9 rounded-xl bg-accent/10 flex items-center justify-center flex-shrink-0">
          <SquareTerminal class="w-4 h-4 text-accent" />
        </div>
        <div class="space-y-1 min-w-0">
          <h2 id="cli-approve-title" class="font-semibold">{{ t('safe.cli.approve_title') }}</h2>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.cli.approve_body') }}</p>
        </div>
      </div>
      <dl class="text-xs space-y-2">
        <div>
          <dt class="text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.command') }}</dt>
          <dd class="font-mono break-all">{{ queue[0].command }}</dd>
        </div>
        <div>
          <dt class="text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.folder') }}</dt>
          <dd class="font-mono break-all">{{ queue[0].cwd }}</dd>
        </div>
        <div>
          <dt class="text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.secrets') }}</dt>
          <dd v-for="s in queue[0].secrets" :key="s" class="font-mono break-all">{{ s }}</dd>
        </div>
      </dl>
      <div class="flex justify-end gap-2">
        <button class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" autofocus @click="answer(false)">{{ t('safe.cli.deny') }}</button>
        <button class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium" @click="answer(true)">{{ t('safe.cli.allow') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
