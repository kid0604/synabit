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
import type { Skill, SkillUsage } from '../types';

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
  const usage = ref<SkillUsage[]>([]);
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  const ns = useNodeService();
  const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      const [rows, used] = await Promise.all([
        invoke<Skill[]>('syn_list_skills'),
        invoke<SkillUsage[]>('syn_skill_usage', { vaultPath: vaultPath() }),
      ]);
      skills.value = rows;
      usage.value = used;
    } catch (e) {
      logger.error('[Syn] Failed to read skills', e);
      error.value = asMessage(e);
      skills.value = [];
      usage.value = [];
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

  return { skills, usage, ordered, isLoading, error, load, setEnabled, usageOf };
}
