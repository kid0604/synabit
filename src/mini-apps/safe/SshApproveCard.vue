<script setup lang="ts">
/**
 * "May `ssh` sign with this key?" — raised by Safe's SSH agent for each
 * signature while "ask each time" is on. It names the key and, for a login,
 * the user it logs in as; no answer within a minute is a no.
 */
import { useI18n } from 'vue-i18n';
import { Terminal } from 'lucide-vue-next';
import AppDialog from '../../shared/components/AppDialog.vue';
import { useApprovalQueue } from './useApprovalQueue';

const { t } = useI18n();

interface Ask {
  id: string;
  timeout_secs?: number;
  request: { key: string; fingerprint: string; login_as: string | null };
}

const { queue, answer } = useApprovalQueue<Ask>('safe://ssh-approve');
</script>

<template>
  <AppDialog
    v-if="queue.length"
    :show="true"
    labelledby="ssh-approve-title"
    size="sm" elevated unstyled panel-class="bg-surface dark:bg-surface-dark text-text dark:text-text-dark rounded-2xl shadow-2xl border border-border dark:border-border-dark flex flex-col overflow-hidden max-h-[90vh]"
    @close="answer(false)">
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
        <!-- Deny has the focus: the card appears on its own, and an Enter meant
             for something else must not sign with a key. -->
        <button class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" autofocus @click="answer(false)">{{ t('safe.ssh.deny') }}</button>
        <button class="btn-primary" @click="answer(true)">{{ t('safe.ssh.allow') }}</button>
      </div>
    </div>
  </AppDialog>
</template>
