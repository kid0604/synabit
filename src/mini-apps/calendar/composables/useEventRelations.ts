import { ref, computed } from 'vue';
import type { Ref } from 'vue';
import type { EventFormData } from '../types';
import { buildEventPayload } from '../helpers';
import { logger } from '../../../utils/logger';
import { openCheckboxes } from '../checkboxes';
import type { NoteCheckbox } from '../checkboxes';
import { i18n } from '../../../i18n';
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

    // ── Who was there ───────────────────────────────────────
    //
    // Attendees are person nodes, not email addresses. That is the whole
    // difference: an address can be shown next to a meeting, but a node can be
    // asked "and when else did we meet?".
    const peopleQuery = ref('');
    const peopleMatches = ref<{ id: string, title: string }[]>([]);
    const isAddingPerson = ref(false);

    const searchPeople = async () => {
        const text = peopleQuery.value.trim();
        if (!text) { peopleMatches.value = []; return; }
        try {
            const found: any[] = await ns.getNodeSummaries('person');
            const needle = text.toLowerCase();
            peopleMatches.value = found
                .filter((p: any) => (p.title || '').toLowerCase().includes(needle))
                .slice(0, 8)
                .map((p: any) => ({ id: p.id, title: p.title }));
        } catch (e) {
            logger.error('Could not look up people:', e);
            peopleMatches.value = [];
        }
    };

    /** The people this event already names. */
    const eventPeople = computed(() =>
        eventRelations.value.filter(r => r.node_type === 'person'));

    const addPerson = async (person: { id: string, title: string }) => {
        const mention = `[${person.title}](synabit://person/${person.id})`;
        eventForm.value.relations = eventForm.value.relations || [];
        if (!eventForm.value.relations.some(link => link.includes(person.id))) {
            eventForm.value.relations.push(mention);
        }
        peopleQuery.value = '';
        peopleMatches.value = [];
        isAddingPerson.value = false;

        // Saved straight away, because the graph edge is what makes the
        // question answerable and a link that only exists in an open form
        // answers nothing.
        if (eventForm.value.isEdit && eventForm.value.path) {
            await ns.writeNode(buildEventPayload(eventForm.value, {
                relPath: eventForm.value.path,
                silent: true,
            }));
            if (eventForm.value.title && eventForm.value.id) {
                await loadEventBacklinks(eventForm.value.title, eventForm.value.id);
            }
        }
    };

    const removePerson = async (id: string) => {
        eventForm.value.relations = (eventForm.value.relations || [])
            .filter(link => !link.includes(id));
        eventBacklinks.value = eventBacklinks.value.filter(n => n.id !== id);
        if (eventForm.value.isEdit && eventForm.value.path) {
            await ns.writeNode(buildEventPayload(eventForm.value, {
                relPath: eventForm.value.path,
                silent: true,
            }));
        }
    };

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
                await ns.writeNode(buildEventPayload(eventForm.value, {
                    relPath: eventForm.value.path,
                    silent: true,
                }));
                await loadEventBacklinks(eventForm.value.title, eventForm.value.id);
            }
        } catch (e) {
            console.error("Failed to create note", e);
        }
    };

    const deleteRelationNode = async (bl: any) => {
        const isConfirmed = await ask(
            i18n.global.t('calendar.delete_relation_body', { type: bl.node_type, title: bl.title }),
            { title: i18n.global.t('calendar.delete_item'), kind: 'warning' },
        );
        if (!isConfirmed) return;
        
        try {
            await ns.deleteNode({ relPath: bl.id });
            
            if (eventForm.value.relations) {
                const originalLength = eventForm.value.relations.length;
                eventForm.value.relations = eventForm.value.relations.filter(link => !link.includes(bl.id));
                if (eventForm.value.relations.length < originalLength && eventForm.value.id) {
                    // Background save without closing modal
                        await ns.writeNode(buildEventPayload(eventForm.value, {
                    relPath: eventForm.value.path,
                    silent: true,
                }));
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

    // ── What came out of the meeting ────────────────────────
    //
    // The boxes somebody ticked into a meeting note, offered as tasks. Read,
    // never inferred: `- [ ] call Anh back` is an action because markdown says
    // so, and a note full of prose produces nothing rather than a guess.
    const noteActions = ref<{ noteId: string; noteTitle: string; box: NoteCheckbox }[]>([]);
    const isMakingTasks = ref(false);

    const loadNoteActions = async () => {
        const notes = eventRelations.value.filter(r => r.node_type === 'note');
        const found: { noteId: string; noteTitle: string; box: NoteCheckbox }[] = [];
        for (const note of notes) {
            try {
                const full = await ns.getNode(note.id);
                const body = typeof full?.content === 'string' ? full.content : '';
                for (const box of openCheckboxes(body)) {
                    found.push({ noteId: note.id, noteTitle: note.title, box });
                }
            } catch (e) {
                logger.error('Could not read a linked note:', e);
            }
        }
        noteActions.value = found;
    };

    /**
     * Turn the chosen boxes into tasks that point back at both the meeting and
     * the note they came out of, so "why is this on my list" has an answer.
     */
    const makeTasksFromNotes = async (chosen: number[]): Promise<number> => {
        if (chosen.length === 0) return 0;
        isMakingTasks.value = true;
        let made = 0;
        try {
            for (const index of chosen) {
                const item = noteActions.value[index];
                if (!item) continue;
                const links = [`[${eventForm.value.title}](synabit://event/${eventForm.value.path})`];
                if (item.noteId) links.push(`[${item.noteTitle}](synabit://note/${item.noteId})`);
                try {
                    await ns.writeNode({
                        relPath: `Tasks/${crypto.randomUUID()}.md`,
                        title: item.box.text,
                        nodeType: 'task',
                        properties: { status: 'todo', relations: links },
                        content: '',
                        eventType: 'created',
                    });
                    made++;
                } catch (e) {
                    logger.error('Could not create a task from a note:', e);
                }
            }
        } finally {
            isMakingTasks.value = false;
        }
        return made;
    };

    const resetEventBacklinks = () => { eventBacklinks.value = []; noteActions.value = []; };
    const resetCreatingNote = () => { isCreatingNote.value = false; };

    return {
        eventBacklinks, isCreatingNote, newNoteTitle,
        eventRelations, eventPeople,
        noteActions, isMakingTasks, loadNoteActions, makeTasksFromNotes,
        peopleQuery, peopleMatches, isAddingPerson, searchPeople, addPerson, removePerson,
        loadEventBacklinks, createMeetingNote, deleteRelationNode, openLinkedNote,
        resetEventBacklinks, resetCreatingNote,
    };
}
