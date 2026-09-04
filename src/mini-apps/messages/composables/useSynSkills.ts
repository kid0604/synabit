/**
 * Reading the skills Syn has, and deciding which of them it may use.
 *
 * Reads go through a typed command, because a skill has a dozen frontmatter
 * keys with defaults and a screen that re-derived those would be a second
 * opinion about what a skill is. The one write — turning a skill on or off —
 * goes through the ordinary node service, because a skill is an ordinary node
 * and that path already has version history, sync and a trash behind it.
 *
 * That last part is not an economy. A skill changes what the assistant *does*,
 * so every change to one should be a diff somebody can read and undo, and
 * `list_versions`/`restore_version` give that away for free.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';
import type { Skill, SkillTrial, SkillUsage } from '../types';

/**
 * May the user turn this on yet?
 *
 * Mirrors `Skill::may_be_enabled`. A skill they wrote is theirs to enable
 * whenever they like — they know what is in it, because they typed it. One Syn
 * wrote has to have answered something both ways first.
 */
export const mayBeEnabled = (skill: Skill): boolean =>
  skill.author !== 'syn' || !!skill.trial_at;

/**
 * How a skill list is read: on first, then by name.
 *
 * Enabled ones first because they are the ones changing behaviour right now,
 * and somebody opening this screen is usually there to check on those or to
 * turn one off. Within a group, by name, so the list does not reshuffle itself
 * when a skill is used.
 */
export const orderSkills = (skills: Skill[]): Skill[] =>
  [...skills].sort((a, b) => {
    const enabled = Number(b.enabled) - Number(a.enabled);
    if (enabled) return enabled;
    return a.name.localeCompare(b.name);
  });

export function useSynSkills(vaultPath: () => string) {
  const skills = ref<Skill[]>([]);
  const trials = ref<Record<string, SkillTrial>>({});
  const trialling = ref<string | null>(null);
  const usage = ref<SkillUsage[]>([]);
  /** What is wrong with each recipe, by skill id. Empty when all is well. */
  const recipeProblems = ref<Record<string, string[]>>({});
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  const ns = useNodeService();
  const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      const [rows, used, problems] = await Promise.all([
        invoke<Skill[]>('syn_list_skills'),
        invoke<SkillUsage[]>('syn_skill_usage', { vaultPath: vaultPath() }),
        invoke<Record<string, string[]>>('syn_recipe_problems'),
      ]);
      skills.value = rows;
      usage.value = used;
      recipeProblems.value = problems;
    } catch (e) {
      logger.error('[Syn] Failed to read skills', e);
      error.value = asMessage(e);
      skills.value = [];
      usage.value = [];
      recipeProblems.value = {};
    } finally {
      isLoading.value = false;
    }
  };

  const ordered = computed(() => orderSkills(skills.value));

  /** How many times a skill has been opened, and when. */
  const usageOf = (skill: Skill): SkillUsage | undefined =>
    usage.value.find(u => u.name.toLowerCase() === skill.name.toLowerCase());

  /**
   * Turn a skill on or off.
   *
   * A property patch: keys not named are left exactly as they are on disk, so
   * saving here cannot delete a field somebody added to the file by hand.
   */
  const setEnabled = async (skill: Skill, enabled: boolean) => {
    error.value = null;
    try {
      await ns.writeNode({
        relPath: skill.id,
        nodeType: 'syn_skill',
        title: skill.title,
        properties: { enabled },
      });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to change whether a skill is enabled', e);
      error.value = asMessage(e);
    }
  };

  /**
   * Take a revision Syn proposed, or leave it.
   *
   * Accepting writes the new steps as a new version, so the one before is a
   * `restore_version` away — which is what makes reading a revision a cheap
   * decision rather than a commitment.
   */
  const decideRevision = async (skill: Skill, accept: boolean) => {
    error.value = null;
    try {
      await ns.writeNode({
        relPath: skill.id,
        nodeType: 'syn_skill',
        title: skill.title,
        content: accept ? (skill.pending_revision ?? skill.body) : undefined,
        properties: accept
          ? { version: skill.version + 1, pending_revision: '', revision_because: '' }
          : { pending_revision: '', revision_because: '' },
      });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to decide on a revision', e);
      error.value = asMessage(e);
    }
  };

  /**
   * Start a skill for the user to write.
   *
   * Creates the file and stops. What goes in it is theirs, and the template
   * carries the documentation, so there is no form here to fill in badly.
   */
  const create = async (name: string) => {
    error.value = null;
    try {
      await invoke<string>('syn_create_skill', { vaultPath: vaultPath(), name });
      await load();
      return true;
    } catch (e) {
      logger.error('[Syn] Failed to start a skill', e);
      error.value = asMessage(e);
      return false;
    }
  };

  /**
   * Run a skill against the question it was invented for, both ways.
   *
   * The result is shown, not scored. Whether one answer is better than the
   * other is a judgement about this person's work, and the app has no business
   * making it for them — the same conclusion the memory eval reached after four
   * separate scorer defects each produced a plausible number.
   */
  const trial = async (skill: Skill) => {
    error.value = null;
    trialling.value = skill.id;
    try {
      trials.value = {
        ...trials.value,
        [skill.id]: await invoke<SkillTrial>('syn_skill_trial', {
          vaultPath: vaultPath(),
          skillId: skill.id,
        }),
      };
      await load();
    } catch (e) {
      logger.error('[Syn] A skill trial failed', e);
      error.value = asMessage(e);
    } finally {
      trialling.value = null;
    }
  };

  return {
    skills, usage, ordered, isLoading, error, trials, trialling, recipeProblems,
    load, setEnabled, usageOf, trial, create, decideRevision,
  };
}
