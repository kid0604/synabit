import { ref, computed, watch } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload } from '../helpers';
import { resolveNoteId } from '../resolveNoteId';
import type { NodeMetadata } from '../../../types/ipc';
import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';

/**
 * The links out of a note, read the way the backend reads them.
 *
 * The destination runs to the closing bracket of the markdown link, not to the
 * first space. Note paths contain spaces — `Notes/công ty cổ phần.md` — and
 * stopping at one captured `Notes/công`, which matched nothing, so a note with
 * a Vietnamese title had no outgoing links at all. This mirrors `MD_LINK_RE`
 * on the Rust side, which is what actually decides the graph.
 */
const OUTGOING_LINK_RE = /\[[^\]]*\]\(synabit:\/\/note\/([^)]+)\)/g;

/** Percent-decode a link target, or leave it alone if it is not encoded. */
const decodeTarget = (raw: string): string => {
  try {
    return decodeURIComponent(raw);
  } catch {
    // A stray `%` is not an encoding, and is no reason to drop the link.
    return raw;
  }
};


export function useNoteBacklinks(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  currentContent: ComputedRef<string>,
  ns: any,
  scanVault: () => Promise<void>,
  /** See `useNoteTags`: unlinking rewrites the body, so it must be current. */
  flushEditor: () => void = () => {},
) {
  const currentBacklinks = ref<NodeMetadata[]>([]);

  const currentOutgoingLinks = computed(() => {
    if (!currentContent.value) return [];
    const links = new Set<string>();
    OUTGOING_LINK_RE.lastIndex = 0;
    let m;
    while ((m = OUTGOING_LINK_RE.exec(currentContent.value)) !== null) {
        const target = decodeTarget(m[1]);

        // The same question the navigation code answers, answered the same
        // way — one link should not open one note and be drawn against
        // another. A target that resolves to nothing is kept as written, so a
        // broken link is drawn as a ghost rather than quietly disappearing.
        links.add(resolveNoteId(notes.value, target)?.id ?? target);
    }
    return Array.from(links);
  });

  const unlinkProject = async (projectId: string, projectTitle?: string) => {
    const isConfirmed = await ask(
        `This note will no longer be linked to "${projectTitle || 'this project'}".`, 
        { 
            title: 'Unlink project?', 
            kind: 'warning',
            okLabel: 'Unlink',
            cancelLabel: 'Cancel'
        }
    );
    if (!isConfirmed) return;

    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note || !note.linked_projects) return;
    
    const linkToRemove = note.linked_projects.find((link: string) => {
        const m = /synabit:\/\/project\/([^\s\)"']+)/.exec(link);
        return m && decodeURIComponent(m[1]) === projectId;
    });

    if (linkToRemove) {
        note.linked_projects = note.linked_projects.filter((l: string) => l !== linkToRemove);
        currentBacklinks.value = currentBacklinks.value.filter(bl => bl.id !== projectId);
        
        flushEditor();
        await ns.writeNode(buildNotePayload(note, currentContent.value));
        scanVault();
    }
  };

  // Watch currentNoteId -> fetch backlinks (only backlinks loading, NOT loadNoteFile)
  watch(currentNoteId, async (newId) => {
    if (newId) {
        try {
            const note = notes.value.find(n => n.id === newId);
            const backlinks = await ns.getLinkedNodes(note?.title || '', newId);
            
            const outgoingProjects: NodeMetadata[] = [];
            const linkedProjects: string[] = (note as any)?.linked_projects || [];
            for (const link of linkedProjects) {
               const m = /synabit:\/\/project\/([^\s\)"']+)/.exec(link);
               if (m && m[1]) {
                   try {
                       const proj = await ns.getNode(decodeURIComponent(m[1]));
                       if (proj) {
                           proj.node_type = 'project';
                           proj._is_outgoing_project = true;
                           outgoingProjects.push(proj);
                       }
                   } catch(e) {}
               }
            }
            
            currentBacklinks.value = [...backlinks, ...outgoingProjects];
        } catch (e) { logger.error(String(e)); currentBacklinks.value = []; }
    } else { currentBacklinks.value = []; }
  });

  return {
    currentBacklinks,
    currentOutgoingLinks,
    unlinkProject,
  };
}
