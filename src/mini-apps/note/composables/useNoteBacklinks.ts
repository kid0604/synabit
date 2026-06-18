import { ref, computed, watch } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload } from '../helpers';
import type { NodeMetadata } from '../../../types/ipc';
import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';

export function useNoteBacklinks(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  currentContent: ComputedRef<string>,
  ns: any,
  scanVault: () => Promise<void>,
) {
  const currentBacklinks = ref<NodeMetadata[]>([]);

  const currentOutgoingLinks = computed(() => {
    if (!currentContent.value) return [];
    const regex = /synabit:\/\/note\/([^\s\)"']+)/g;
    const links = new Set<string>();
    let m;
    while ((m = regex.exec(currentContent.value)) !== null) {
        const targetFilename = decodeURIComponent(m[1]);
        const targetNote = notes.value.find(n => n.path.endsWith(targetFilename));
        if (targetNote) links.add(targetNote.id);
        else links.add(targetFilename);
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
            
            let outgoingProjects: NodeMetadata[] = [];
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
