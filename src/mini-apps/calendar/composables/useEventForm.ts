import { ref, computed, watch } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type { EventMetadata, EventFormData } from '../types';
import { minuteOptions, formatDateString, parseTags } from '../helpers';
import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';

export function useEventForm(
    allEvents: Ref<EventMetadata[]>,
    ns: any,
    selectedDateFormattedStr: ComputedRef<string>,
    loadData: () => Promise<void>,
    loadEventBacklinks: (title: string, id: string) => Promise<void>,
    resetEventBacklinks: () => void,
    resetCreatingNote: () => void,
) {
    const showEventForm = ref(false);
    const eventForm = ref<EventFormData>({
        isEdit: false,
        id: '',
        path: '',
        title: '',
        isAllDay: false,
        start_at: '',
        end_at: '',
        location: '',
        description: '',
        tagsStr: '',
        relations: [] as string[],
        recurrence: 'none',
        recurrence_end_at: '',
        series_id: '',
        exceptions: [] as string[],
        reminders: [] as string[],
        _editScope: 'all' as 'occurrence_view' | 'this' | 'following' | 'all',
        _originalEvent: null as EventMetadata | null
    });

    // Scope modal state
    const showScopeModal = ref(false);
    const scopeAction = ref<'edit' | 'delete'>('edit');
    const scopeSelection = ref<'this' | 'following' | 'all'>('this');
    const targetOccurrenceDate = ref('');
    const pendingEventAction = ref<EventMetadata | null>(null);

    // --- Time picking computeds ---
    const startAtDate = computed({
        get: () => eventForm.value.start_at.split('T')[0],
        set: (v) => eventForm.value.start_at = `${v}T${eventForm.value.start_at.split('T')[1] || '12:00'}`
    });
    const startAtHour = computed({
        get: () => (eventForm.value.start_at.split('T')[1] || '12:00').split(':')[0],
        set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || new Date().toISOString().split('T')[0]}T${v}:${startAtMinute.value}`
    });
    const startAtMinute = computed({
        get: () => (eventForm.value.start_at.split('T')[1] || '12:00').split(':')[1],
        set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || new Date().toISOString().split('T')[0]}T${startAtHour.value}:${v}`
    });
    const startAtMinuteOptions = computed(() => {
        const opts = [...minuteOptions];
        if (startAtMinute.value && !opts.includes(startAtMinute.value)) {
            opts.push(startAtMinute.value);
            opts.sort();
        }
        return opts;
    });

    const endAtDate = computed({
        get: () => (eventForm.value.end_at || '').split('T')[0],
        set: (v) => eventForm.value.end_at = `${v}T${(eventForm.value.end_at || '').split('T')[1] || '13:00'}`
    });
    const endAtHour = computed({
        get: () => (eventForm.value.end_at || 'T13:00').split('T')[1].split(':')[0],
        set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || new Date().toISOString().split('T')[0]}T${v}:${endAtMinute.value}`
    });
    const endAtMinute = computed({
        get: () => (eventForm.value.end_at || 'T13:00').split('T')[1].split(':')[1],
        set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || new Date().toISOString().split('T')[0]}T${endAtHour.value}:${v}`
    });
    const endAtMinuteOptions = computed(() => {
        const opts = [...minuteOptions];
        if (endAtMinute.value && !opts.includes(endAtMinute.value)) {
            opts.push(endAtMinute.value);
            opts.sort();
        }
        return opts;
    });

    // --- Reminders ---
    const reminderPreset = ref('');
    const customReminder = ref('');
    const addReminder = () => {
        let val = '';
        if (reminderPreset.value === 'custom') {
            if (customReminder.value) {
                val = customReminder.value.trim().toLowerCase();
                if (!val.match(/^\d+[mhd]$/)) {
                    alert("Custom reminder must be a number followed by m, h, or d (e.g., 45m, 2h, 1d)");
                    return;
                }
            }
        } else if (reminderPreset.value) {
            val = reminderPreset.value;
        }
        if (val && !eventForm.value.reminders.includes(val)) {
            eventForm.value.reminders.push(val);
        }
        reminderPreset.value = '';
        customReminder.value = '';
    };
    const removeReminder = (idx: number) => {
        eventForm.value.reminders.splice(idx, 1);
    };

    // --- isAllDay watcher ---
    watch(() => eventForm.value.isAllDay, (newVal) => {
        if (newVal) {
            eventForm.value.start_at = eventForm.value.start_at.split('T')[0];
            if (eventForm.value.end_at) {
                eventForm.value.end_at = eventForm.value.end_at.split('T')[0];
            }
        } else {
            if (!eventForm.value.start_at.includes('T')) {
                eventForm.value.start_at = `${eventForm.value.start_at}T12:00:00`;
            }
            if (eventForm.value.end_at && !eventForm.value.end_at.includes('T')) {
                eventForm.value.end_at = `${eventForm.value.end_at}T13:00:00`;
            }
        }
    });

    // --- Modal open/close ---
    const openAddEventModal = (defaultDate?: Date, hr?: number) => {
        const targetDateStr = defaultDate ? formatDateString(defaultDate) : selectedDateFormattedStr.value;
        const startHour = hr !== undefined ? hr.toString().padStart(2, '0') : '12';
        const endHour = hr !== undefined ? (hr + 1).toString().padStart(2, '0') : '13';
        eventForm.value = {
            isEdit: false, id: '', path: '', title: '',
            isAllDay: false, start_at: `${targetDateStr}T${startHour}:00`, end_at: `${targetDateStr}T${endHour}:00`,
            location: '', description: '', tagsStr: '', relations: [] as string[],
            recurrence: 'none', recurrence_end_at: '', series_id: '', exceptions: [], reminders: [], _editScope: 'all', _originalEvent: null
        };
        resetEventBacklinks();
        resetCreatingNote();
        showEventForm.value = true;
    };

    const openEditEventModal = (ev: EventMetadata, dateStr: string) => {
        targetOccurrenceDate.value = dateStr;
        pendingEventAction.value = ev;
        openEditEventModalActual(ev, dateStr, 'occurrence_view');
    };

    const openEditEventModalActual = async (ev: EventMetadata, dateStr: string, scope: 'occurrence_view' | 'this' | 'following' | 'all') => {
        let startAt = ev.start_at || '';
        if (startAt.includes('T')) startAt = startAt.slice(0, 16);
        let endAt = ev.end_at || '';
        if (endAt.includes('T')) endAt = endAt.slice(0, 16);
        
        if (scope === 'occurrence_view' || scope === 'this' || scope === 'following') {
            const timePartStart = startAt.includes('T') ? startAt.split('T')[1] : '12:00';
            const timePartEnd = endAt.includes('T') ? endAt.split('T')[1] : '13:00';
            startAt = `${dateStr}T${timePartStart}`;
            endAt = `${dateStr}T${timePartEnd}`;
        }
        
        eventForm.value = {
            isEdit: true, id: ev.id, path: ev.path, title: ev.title,
            isAllDay: ev.is_all_day, start_at: startAt, end_at: endAt, location: ev.location,
            description: ev.content, tagsStr: ev.tags.join(', '),
            relations: [...(ev.relations || [])],
            recurrence: ev.recurrence || 'none',
            recurrence_end_at: ev.recurrence_end_at || '',
            series_id: ev.series_id || '',
            exceptions: [...(ev.exceptions || [])],
            reminders: [...(ev.reminders || [])],
            _editScope: scope as any,
            _originalEvent: ev
        };
        resetEventBacklinks();
        resetCreatingNote();
        showEventForm.value = true;
        if (ev.title && ev.id) {
            loadEventBacklinks(ev.title, ev.id);
        }
    };

    const closeEventForm = () => { showEventForm.value = false; };

    // --- Submit ---
    const submitEvent = async () => {
        if (!eventForm.value.title || !eventForm.value.start_at) return;
        
        if (eventForm.value.isEdit && eventForm.value._originalEvent && eventForm.value._originalEvent.recurrence && eventForm.value._originalEvent.recurrence !== 'none') {
            if (eventForm.value._editScope === 'occurrence_view') {
                scopeAction.value = 'edit';
                scopeSelection.value = 'this';
                showScopeModal.value = true;
                return;
            }
        }
        
        await submitEventActual();
    };

    const submitEventActual = async () => {
        const finalTags = parseTags(eventForm.value.tagsStr);
        
        // Normalize format to drop seconds or keep ISO consistent if desired, but HTML datetime-local uses YYYY-MM-DDTHH:mm
        
        try {
            let relPath = eventForm.value.path;
            let isCreatingNewNode = !eventForm.value.isEdit || !relPath;
            
            const properties: any = {
                is_all_day: eventForm.value.isAllDay,
                start_at: eventForm.value.start_at,
                end_at: eventForm.value.end_at,
                location: eventForm.value.location,
                tags: finalTags,
                recurrence: eventForm.value.recurrence,
                recurrence_end_at: eventForm.value.recurrence_end_at,
                series_id: eventForm.value.series_id,
                exceptions: eventForm.value.exceptions,
                reminders: eventForm.value.reminders
            };
            
            if (eventForm.value.isEdit && eventForm.value._editScope === 'all' && eventForm.value._originalEvent) {
                const parentEv = eventForm.value._originalEvent;
                const rootId = parentEv.series_id || parentEv.id;
                const rootEv = allEvents.value.find(e => e.id === rootId) || parentEv;
                
                const origStart = rootEv.start_at || '';
                const origEnd = rootEv.end_at || '';
                const origStartDate = origStart.split('T')[0];
                const origEndDate = origEnd.split('T')[0];
                
                const occurrenceStartObj = new Date(targetOccurrenceDate.value + 'T00:00:00');
                const newStartObj = new Date(eventForm.value.start_at.split('T')[0] + 'T00:00:00');
                const diffMs = newStartObj.getTime() - occurrenceStartObj.getTime();
                
                const rootStartObj = new Date(origStartDate + 'T00:00:00');
                rootStartObj.setTime(rootStartObj.getTime() + diffMs);
                const shiftedOrigStartDate = rootStartObj.toISOString().split('T')[0];
                
                const rootEndObj = new Date(origEndDate + 'T00:00:00');
                rootEndObj.setTime(rootEndObj.getTime() + diffMs);
                const shiftedOrigEndDate = rootEndObj.toISOString().split('T')[0];
                
                const newTimeStart = eventForm.value.start_at.includes('T') ? eventForm.value.start_at.split('T')[1] : '';
                const newTimeEnd = eventForm.value.end_at.includes('T') ? eventForm.value.end_at.split('T')[1] : '';
                
                properties.start_at = newTimeStart ? `${shiftedOrigStartDate}T${newTimeStart}` : shiftedOrigStartDate;
                properties.end_at = newTimeEnd ? `${shiftedOrigEndDate}T${newTimeEnd}` : shiftedOrigEndDate;
                
                properties.exceptions = [];
                properties.series_id = '';
                
                const familyEvents = allEvents.value.filter(e => e.id === rootId || e.series_id === rootId);
                let maxEndAt = '';
                let isInfinite = false;
                for (const fam of familyEvents) {
                    if (fam.recurrence && fam.recurrence !== 'none') {
                        if (!fam.recurrence_end_at) {
                            isInfinite = true;
                            break;
                        } else if (fam.recurrence_end_at > maxEndAt) {
                            maxEndAt = fam.recurrence_end_at;
                        }
                    }
                }
                if (!eventForm.value.recurrence_end_at || eventForm.value.recurrence_end_at === parentEv.recurrence_end_at) {
                    properties.recurrence_end_at = isInfinite ? '' : maxEndAt;
                }
                
                for (const famEv of familyEvents) {
                    if (famEv.path !== rootId) {
                        await ns.deleteNode({ relPath: famEv.path, silent: true });
                    }
                }
                
                relPath = rootId;
            }
            
            if (eventForm.value.relations && eventForm.value.relations.length > 0) {
                properties.relations = eventForm.value.relations;
            }

            if (eventForm.value.isEdit && (eventForm.value._editScope === 'this' || eventForm.value._editScope === 'following')) {
                relPath = `Events/${crypto.randomUUID()}.md`;
                isCreatingNewNode = true;
                
                const parentEv = eventForm.value._originalEvent!;
                properties.recurrence = eventForm.value._editScope === 'this' ? 'none' : eventForm.value.recurrence;
                properties.recurrence_end_at = eventForm.value._editScope === 'this' ? '' : eventForm.value.recurrence_end_at;
                properties.series_id = parentEv.series_id || parentEv.id;
                properties.exceptions = []; // New split event should not inherit exceptions
                const parentProps = {
                    is_all_day: parentEv.is_all_day,
                    start_at: parentEv.start_at,
                    end_at: parentEv.end_at,
                    location: parentEv.location,
                    tags: parentEv.tags,
                    recurrence: parentEv.recurrence,
                    recurrence_end_at: parentEv.recurrence_end_at,
                    exceptions: [...(parentEv.exceptions || [])],
                    relations: [...(parentEv.relations || [])],
                    series_id: parentEv.series_id
                };
                
                if (eventForm.value._editScope === 'this') {
                    if (!parentProps.exceptions.includes(targetOccurrenceDate.value)) {
                        parentProps.exceptions.push(targetOccurrenceDate.value);
                    }
                } else if (eventForm.value._editScope === 'following') {
                    const dt = new Date(targetOccurrenceDate.value + 'T00:00:00');
                    dt.setDate(dt.getDate() - 1);
                    parentProps.recurrence_end_at = dt.toISOString().split('T')[0];
                }
                
                await ns.writeNode({
                    relPath: parentEv.path,
                    title: parentEv.title,
                    nodeType: 'event',
                    properties: parentProps,
                    content: parentEv.content,
                    silent: true,
                });
            }
            
            if (isCreatingNewNode) {
                if (!relPath) relPath = `Events/${crypto.randomUUID()}.md`;
            }
            
            await ns.writeNode({ 
                relPath,
                title: eventForm.value.title,
                nodeType: 'event',
                properties,
                content: eventForm.value.description,
                eventType: isCreatingNewNode ? 'created' : 'updated',
            });
            closeEventForm();
            await loadData();
        } catch(e) { logger.error("Failed to save event:", e); }
    };

    // --- Delete ---
    const deleteEvent = async (ev: EventMetadata, dateStr: string) => {
        if (ev.recurrence && ev.recurrence !== 'none') {
            scopeAction.value = 'delete';
            scopeSelection.value = 'this';
            targetOccurrenceDate.value = dateStr;
            pendingEventAction.value = ev;
            showScopeModal.value = true;
        } else {
            const isConfirmed = await ask('This action cannot be undone. The event will be permanently removed from your calendar.', { 
                title: `Delete event '${ev.title}'?`, 
                kind: 'warning',
                okLabel: 'Delete',
                cancelLabel: 'Cancel'
            });
            if (isConfirmed) {
                await deleteEventActual(ev, dateStr, 'all');
            }
        }
    };

    const deleteEventActual = async (ev: EventMetadata, dateStr: string, scope: 'this' | 'following' | 'all') => {
        try {
            if (scope === 'all') {
                const rootId = ev.series_id || ev.id;
                const familyEvents = allEvents.value.filter(e => e.id === rootId || e.series_id === rootId);
                for (const famEv of familyEvents) {
                    if (famEv.path !== ev.path) {
                        await ns.deleteNode({ relPath: famEv.path, silent: true });
                    }
                }
                await ns.deleteNode({ relPath: ev.path });
            } else {
                const parentProps = {
                    is_all_day: ev.is_all_day,
                    start_at: ev.start_at,
                    end_at: ev.end_at,
                    location: ev.location,
                    tags: ev.tags,
                    recurrence: ev.recurrence,
                    recurrence_end_at: ev.recurrence_end_at,
                    exceptions: [...(ev.exceptions || [])],
                    relations: [...(ev.relations || [])],
                    series_id: ev.series_id
                };
                
                if (scope === 'this') {
                    if (!parentProps.exceptions.includes(dateStr)) {
                        parentProps.exceptions.push(dateStr);
                    }
                } else if (scope === 'following') {
                    const dt = new Date(dateStr + 'T00:00:00');
                    dt.setDate(dt.getDate() - 1);
                    parentProps.recurrence_end_at = dt.toISOString().split('T')[0];
                }
                
                await ns.writeNode({
                    relPath: ev.path,
                    title: ev.title,
                    nodeType: 'event',
                    properties: parentProps,
                    content: ev.content,
                });
            }
            await loadData();
        } catch(e) { logger.error("Failed to delete event:", e); }
    };

    const handleDeleteFromForm = () => {
        if (eventForm.value._originalEvent) {
            deleteEvent(eventForm.value._originalEvent, targetOccurrenceDate.value);
            closeEventForm();
        }
    };

    // --- Scope modal ---
    const confirmScopeAction = () => {
        showScopeModal.value = false;
        if (scopeAction.value === 'edit') {
            eventForm.value._editScope = scopeSelection.value as any;
            submitEventActual();
        } else {
            deleteEventActual(pendingEventAction.value!, targetOccurrenceDate.value, scopeSelection.value);
        }
    };

    return {
        showEventForm, eventForm,
        showScopeModal, scopeAction, scopeSelection, targetOccurrenceDate, pendingEventAction,
        startAtDate, startAtHour, startAtMinute, startAtMinuteOptions,
        endAtDate, endAtHour, endAtMinute, endAtMinuteOptions,
        reminderPreset, customReminder, addReminder, removeReminder,
        openAddEventModal, openEditEventModal, closeEventForm,
        submitEvent, deleteEvent, handleDeleteFromForm, confirmScopeAction,
    };
}
