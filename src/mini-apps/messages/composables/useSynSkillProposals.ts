/**
 * Skills Syn wrote for itself, waiting for the person to keep or not.
 *
 * Like `useSynChanges`: not a stop. The run has answered; the skill it wrote is
 * switched off until the person keeps it on the card under that answer. Read
 * back from `syn_waiting_skill_proposals`, so one written while nobody was
 * looking is still there to answer. See `syn::drafting` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export interface SkillCall {
  tool: string;
  does: 'read' | 'write' | 'structural' | 'browse' | 'other';
}

export interface SkillProposal {
  skill_id: string;
  conversation_id?: string | null;
  name: string;
  description: string;
  when_to_use: string;
  tier: 'prose' | 'recipe' | 'code';
  steps: string;
  calls: SkillCall[];
  read_before: string[];
  version: number;
}

const waiting = ref<Record<string, SkillProposal>>({});
let listening = false;

export const refreshSkillProposals = async () => {
  try {
    const found = await invoke<SkillProposal[]>('syn_waiting_skill_proposals');
    const next: Record<string, SkillProposal> = {};
    for (const p of Array.isArray(found) ? found : []) if (p?.skill_id) next[p.skill_id] = p;
    waiting.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the skills waiting to be kept', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<{ conversation_id?: string | null; skills: SkillProposal[] }>('syn-skills-proposed', event => {
    const next = { ...waiting.value };
    for (const p of event.payload?.skills ?? []) if (p?.skill_id) next[p.skill_id] = p;
    waiting.value = next;
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for skills Syn wrote', e);
  });
};

export function useSynSkillProposals(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  if (first) void Promise.resolve().then(refreshSkillProposals);
  const error = ref<string | null>(null);
  const answering = ref(false);

  const proposedIn = (conversationId: string | null | undefined): SkillProposal[] =>
    conversationId ? Object.values(waiting.value).filter(p => p.conversation_id === conversationId) : [];

  /** Keep it (switched on from now) or let it go (to the trash). */
  const answer = async (skillId: string, keep: boolean): Promise<boolean> => {
    if (!waiting.value[skillId] || answering.value) return false;
    answering.value = true;
    error.value = null;
    try {
      await invoke('syn_answer_skill_proposal', { vaultPath: vaultPath(), skillId, keep });
      const { [skillId]: _done, ...rest } = waiting.value;
      waiting.value = rest;
      return true;
    } catch (e) {
      logger.error('[Syn] Could not answer a skill Syn wrote', e);
      error.value = errorText(e);
      return false;
    } finally {
      answering.value = false;
    }
  };

  return { proposedIn, answer, error, answering, refresh: refreshSkillProposals };
}
