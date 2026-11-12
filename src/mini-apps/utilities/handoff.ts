/**
 * The way into Utilities from another app: "Utilities ▸ Merge PDF" on files
 * in Files.
 *
 * The files are named by path — Files knows them by path — and Rust checks
 * each is in the vault or a folder added as a source before taking a copy
 * (`utility_add_from_vault`). One from inside the vault remembers its folder,
 * so the result can be saved beside it.
 *
 * The workbench is imported only when this runs, so opening Files does not
 * load Utilities' screens or pdf.js along with it.
 */
import { ref } from 'vue';
import type { Router } from 'vue-router';
import { addFromVault, officeStatus, visionStatus } from './api';
import { availability, kindOf, suggestionsFor, type OfficeStatus, type ToolDef, type ToolId, type VisionAbilities } from './tools';

/** What Vision can do here, asked once, for leaving out tools that cannot run. */
const vision = ref<VisionAbilities | null | undefined>(undefined);
const office = ref<OfficeStatus | null | undefined>(undefined);
let asked = false;
export function askVision(): void {
  if (asked) return;
  asked = true;
  visionStatus().then((v) => (vision.value = v ?? null)).catch(() => (vision.value = null));
  officeStatus().then((o) => (office.value = o ?? null)).catch(() => (office.value = null));
}

/** The tools to offer for these file names, best first; none for a mix. */
export function toolsForFiles(names: string[], limit = 4): ToolDef[] {
  const kinds = new Set(names.map(kindOf));
  if (kinds.size !== 1) return [];
  const [kind] = [...kinds];
  if (!kind) return [];
  return suggestionsFor(kind, names.length).filter((t) => availability(t, vision.value, office.value) === 'ready').slice(0, limit);
}

export async function openInUtilities(router: Router, vaultPath: string, tool: ToolId, paths: string[]): Promise<void> {
  const inputs = await addFromVault(vaultPath, paths);
  const { useWorkbench } = await import('./useWorkbench');
  useWorkbench().openTool(tool, inputs);
  await router.push({ name: 'utilities', query: { tool } });
}
