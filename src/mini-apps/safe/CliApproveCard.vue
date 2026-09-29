<script setup lang="ts">
/**
 * "May this command have these secrets?" — raised by `synabit-safe run`. It
 * names the command with every argument, the folder it says it runs in, and
 * each variable with the item and field it would get. A request naming an
 * item that cannot be found can only be dismissed. No answer within a minute
 * is a no.
 */
import { useI18n } from 'vue-i18n';
import { SquareTerminal } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';
import { useApprovalQueue } from './useApprovalQueue';

const { t } = useI18n();

interface Ask {
  id: string;
  timeout_secs?: number;
  command: string;
  cwd: string;
  secrets: { name: string; reference: string; item: string | null; problem: string | null }[];
  blocked: boolean;
}

const { queue, answer } = useApprovalQueue<Ask>('safe://cli-approve');
</script>

<template>
  <ModalDialog
    v-if="queue.length"
    :show="true"
    labelled-by="cli-approve-title"
    card-class="max-w-[520px] text-text dark:text-text-dark"
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
          <dd class="font-mono break-all max-h-40 overflow-y-auto">{{ queue[0].command }}</dd>
        </div>
        <div>
          <dt class="text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.folder') }}</dt>
          <dd class="font-mono break-all">{{ queue[0].cwd }}</dd>
        </div>
        <div>
          <dt class="text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.secrets') }}</dt>
          <dd v-for="s in queue[0].secrets" :key="s.name" class="break-all">
            <span class="font-mono">{{ s.name }}</span> ←
            <span v-if="s.item" class="font-medium">{{ s.item }}</span>
            <span v-else class="text-danger">{{ s.reference }} — {{ s.problem }}</span>
          </dd>
        </div>
      </dl>
      <p class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.cli.approve_caveat') }}</p>
      <div class="flex justify-end gap-2">
        <button class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" autofocus @click="answer(false)">{{ t('safe.cli.deny') }}</button>
        <button v-if="!queue[0].blocked" class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium" @click="answer(true)">{{ t('safe.cli.allow') }}</button>
      </div>
    </div>
  </ModalDialog>
</template>
