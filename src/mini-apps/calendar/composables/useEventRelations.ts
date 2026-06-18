import { ref, computed } from 'vue';
import type { Ref } from 'vue';
import type { EventFormData } from '../types';
import { parseTags } from '../helpers';
import { ask } from '@tauri-apps/plugin-dialog';

export function useEventRelations(
    ns: any,
    eventForm: Ref<EventFormData>,
    closeEventForm: () => void,
    emit: (e: 'open-node', id: string, type: string) => void,
) {
    const eventBacklinks = ref<{ id: string, title: string, node_type: string }[]>([]);
    const isCreatingNote = ref(false);
    const newNoteTitle = ref('');

    const loadEventBacklinks = async (title: string, id: string) => {
        try {
            eventBacklinks.value = await ns.getLinkedNodes(title, id);
        } catch (e) {
            console.error("Failed to load event backlinks", e);
            eventBacklinks.value = [];
        }
    };

    const eventRelations = computed(() => {
        const items = [...eventBacklinks.value];
        if (eventForm.value.relations && eventForm.value.relations.length > 0) {
            const mdLinkRe = /\[([^\]]+)\]\(synabit:\/\/(note|node|person|task|quickcap|event)\/([^)]+)\)/;
            for (const link of eventForm.value.relations) {
                const match = mdLinkRe.exec(link);
                if (match) {
                    const title = match[1];
                    const type = match[2];
                    const id = match[3];
                    if (!items.find(n => n.id === id)) {
                        items.push({ id, title, node_type: type });
                    }
                }
            }
        }
        return items;
    });

    const createMeetingNote = async () => {
        if (!newNoteTitle.value.trim() || !eventForm.value.title) return;
        try {
            const relPath = `Notes/note_${Date.now()}.md`;
            await ns.writeNode({
                relPath,
                nodeType: 'note',
                title: newNoteTitle.value.trim(),
                properties: {},
                content: ``,
                eventType: 'created',
            });
            
            const noteMention = `[${newNoteTitle.value.trim()}](synabit://note/${relPath})`;
            eventForm.value.relations = eventForm.value.relations || [];
            eventForm.value.relations.push(noteMention);
            
            isCreatingNote.value = false;
            newNoteTitle.value = '';
            
            if (eventForm.value.id && eventForm.value.path) {
                // Auto-save the event so the graph edge is created immediately
                const finalTags = parseTags(eventForm.value.tagsStr);
                await ns.writeNode({ 
                    relPath: eventForm.value.path,
                    title: eventForm.value.title,
                    nodeType: 'event',
                    properties: {
                        is_all_day: eventForm.value.isAllDay,
                        start_at: eventForm.value.start_at,
                        end_at: eventForm.value.end_at,
                        location: eventForm.value.location,
                        tags: finalTags,
                        relations: eventForm.value.relations || [],
                        recurrence: eventForm.value.recurrence,
                        recurrence_end_at: eventForm.value.recurrence_end_at,
                        series_id: eventForm.value.series_id,
                        exceptions: eventForm.value.exceptions
                    },
                    content: eventForm.value.description,
                    silent: true,
                });
                await loadEventBacklinks(eventForm.value.title, eventForm.value.id);
            }
        } catch (e) {
            console.error("Failed to create note", e);
        }
    };

    const deleteRelationNode = async (bl: any) => {
        const isConfirmed = await ask(`This will permanently delete the ${bl.node_type} "${bl.title}". This action cannot be undone.`, { 
            title: 'Delete Item', 
            kind: 'warning' 
        });
        if (!isConfirmed) return;
        
        try {
            await ns.deleteNode({ relPath: bl.id });
            
            if (eventForm.value.relations) {
                const originalLength = eventForm.value.relations.length;
                eventForm.value.relations = eventForm.value.relations.filter(link => !link.includes(bl.id));
                if (eventForm.value.relations.length < originalLength && eventForm.value.id) {
                    // Background save without closing modal
                    const finalTags = parseTags(eventForm.value.tagsStr);
                    await ns.writeNode({ 
                        relPath: eventForm.value.path,
                        title: eventForm.value.title,
                        nodeType: 'event',
                        properties: {
                            is_all_day: eventForm.value.isAllDay,
                            start_at: eventForm.value.start_at,
                            end_at: eventForm.value.end_at,
                            location: eventForm.value.location,
                            tags: finalTags,
                            relations: eventForm.value.relations,
                            recurrence: eventForm.value.recurrence,
                            recurrence_end_at: eventForm.value.recurrence_end_at,
                            series_id: eventForm.value.series_id,
                            exceptions: eventForm.value.exceptions
                        },
                        content: eventForm.value.description,
                        silent: true,
                    });
                }
            }
            eventBacklinks.value = eventBacklinks.value.filter(n => n.id !== bl.id);
        } catch (e) {
            console.error(`Failed to delete ${bl.node_type}:`, e);
        }
    };

    const openLinkedNote = (id: string, type: string) => {
        closeEventForm();
        emit('open-node', id, type);
    };

    const resetEventBacklinks = () => { eventBacklinks.value = []; };
    const resetCreatingNote = () => { isCreatingNote.value = false; };

    return {
        eventBacklinks, isCreatingNote, newNoteTitle,
        eventRelations,
        loadEventBacklinks, createMeetingNote, deleteRelationNode, openLinkedNote,
        resetEventBacklinks, resetCreatingNote,
    };
}
