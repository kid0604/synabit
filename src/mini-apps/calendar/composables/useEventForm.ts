import { ref, computed, watch } from 'vue';
import type { ComputedRef } from 'vue';
import type { EventMetadata, EventFormData } from '../types';
import { minuteOptions, formatDateString, parseTags, shiftDateString, daysBetween } from '../helpers';
import { defaultRecurrence, ruleOf, isSeries, endingOn, recurrenceProperties } from '../rrule';
import { localTimeZone } from '../timezone';
import { isSubscribed } from '../subscriptions';
import { i18n } from '../../../i18n';
import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';

export function useEventForm(
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
        title: '',
        isAllDay: false,
        tzid: '',
        colour: '',
        start_at: '',
        end_at: '',
        location: '',
        description: '',
        tagsStr: '',
        relations: [] as string[],
        recurrence: defaultRecurrence(),
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
        set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || formatDateString(new Date())}T${v}:${startAtMinute.value}`
    });
    const startAtMinute = computed({
        get: () => (eventForm.value.start_at.split('T')[1] || '12:00').split(':')[1],
        set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || formatDateString(new Date())}T${startAtHour.value}:${v}`
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
        set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || formatDateString(new Date())}T${v}:${endAtMinute.value}`
    });
    const endAtMinute = computed({
        get: () => (eventForm.value.end_at || 'T13:00').split('T')[1].split(':')[1],
        set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || formatDateString(new Date())}T${endAtHour.value}:${v}`
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
    const reminderError = ref('');

    const addReminder = () => {
        // Choosing "Custom…" is a request for the text box, not an attempt to
        // add anything. The select fires this in the same tick as the change,
        // so clearing the preset here used to close the box before it ever
        // rendered — which left custom reminders permanently unreachable.
        if (reminderPreset.value === 'custom' && !customReminder.value.trim()) {
            reminderError.value = '';
            return;
        }

        let val = '';
        if (reminderPreset.value === 'custom') {
            val = customReminder.value.trim().toLowerCase();
            if (!/^\d+[mhd]$/.test(val)) {
                reminderError.value = i18n.global.t('calendar.err_reminder_format');
                return;
            }
        } else if (reminderPreset.value) {
            val = reminderPreset.value;
        }

        if (val && !eventForm.value.reminders.includes(val)) {
            eventForm.value.reminders.push(val);
        }
        reminderPreset.value = '';
        customReminder.value = '';
        reminderError.value = '';
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
    /**
     * `startAt`/`endAt` come from drawing a range on the time grid, which is
     * the only caller that already knows the minutes it wants. Everything else
     * gets a whole hour.
     */
    const openAddEventModal = (defaultDate?: Date, hr?: number, startAt?: string, endAt?: string) => {
        const targetDateStr = defaultDate ? formatDateString(defaultDate) : selectedDateFormattedStr.value;
        const startHour = hr !== undefined ? hr.toString().padStart(2, '0') : '12';
        const endHour = hr !== undefined ? (hr + 1).toString().padStart(2, '0') : '13';
        eventForm.value = {
            isEdit: false, id: '', title: '',
            isAllDay: false,
            tzid: '',
            colour: '',
            start_at: startAt || `${targetDateStr}T${startHour}:00`,
            end_at: endAt || `${targetDateStr}T${endHour}:00`,
            location: '', description: '', tagsStr: '', relations: [] as string[],
            recurrence: defaultRecurrence(), series_id: '', exceptions: [], reminders: [], _editScope: 'all', _originalEvent: null
        };
        resetEventBacklinks();
        resetCreatingNote();
        showErrors.value = false;
        reminderError.value = '';
        showEventForm.value = true;
    };

    const openEditEventModal = (ev: EventMetadata, dateStr: string) => {
        targetOccurrenceDate.value = dateStr;
        pendingEventAction.value = ev;
        return openEditEventModalActual(ev, dateStr, 'occurrence_view');
    };

    /**
     * The form for one occurrence.
     *
     * The occurrence's own times are used as they are. They used to be rebuilt
     * from the clicked day plus the series' time-of-day, because an event
     * handed to the grid carried the *first* occurrence's timestamps and
     * nothing else. Expansion now materialises every instance, so rebuilding
     * would be the thing introducing an error: on the second day of a
     * multi-day instance it moved the start onto the wrong day.
     */
    const formFromEvent = (
        ev: EventMetadata,
        scope: 'occurrence_view' | 'this' | 'following' | 'all',
    ): EventFormData => {
        let startAt = ev.start_at || '';
        if (startAt.includes('T')) startAt = startAt.slice(0, 16);
        let endAt = ev.end_at || '';
        if (endAt.includes('T')) endAt = endAt.slice(0, 16);

        return {
            isEdit: true, id: ev.id, title: ev.title,
            isAllDay: ev.is_all_day, tzid: ev.tzid || '', colour: ev.colour || '',
            start_at: startAt, end_at: endAt, location: ev.location,
            description: ev.content ?? '', tagsStr: (ev.tags || []).join(', '),
            relations: [...(ev.relations || [])],
            recurrence: ruleOf(ev),
            series_id: ev.series_id || '',
            exceptions: [...(ev.exceptions || [])],
            reminders: [...(ev.reminders || [])],
            _editScope: scope,
            _originalEvent: ev,
        };
    };

    const openEditEventModalActual = async (ev: EventMetadata, dateStr: string, scope: 'occurrence_view' | 'this' | 'following' | 'all') => {
        void dateStr;
        eventForm.value = formFromEvent(ev, scope);
        resetEventBacklinks();
        resetCreatingNote();
        showErrors.value = false;
        reminderError.value = '';
        showEventForm.value = true;
        if (ev.title && ev.id) {
            loadEventBacklinks(ev.title, ev.id);
        }

        // A range query carries no bodies, so the description arrives a beat
        // later. The form opens first — waiting on a round trip to show a
        // dialog the user already asked for is the wrong trade — and the text
        // is filled in when it lands, unless they have started typing or moved
        // on to a different event in the meantime.
        if (ev.id) {
            try {
                const full = await ns.getNode(ev.id);
                const body = typeof full?.content === 'string' ? full.content : '';
                const stillTheSameForm = eventForm.value.id === ev.id
                    && eventForm.value.description === (ev.content ?? '');
                if (body && stillTheSameForm) eventForm.value.description = body;
            } catch (e) { logger.error('Could not load the event body:', e); }
        }
    };

    const closeEventForm = () => {
        showEventForm.value = false;
        showErrors.value = false;
        reminderError.value = '';
    };

    // --- Validation ---
    /**
     * Why this is a computed rather than a check inside `submitEvent`: the
     * old code returned silently when the title was missing, so pressing Save
     * on an incomplete form did nothing at all and said nothing about why. An
     * event that ends before it starts had no check whatsoever and saved.
     */
    const formError = computed(() => {
        const f = eventForm.value;
        if (!f.title.trim()) return i18n.global.t('calendar.err_title_required');
        if (!f.start_at) return i18n.global.t('calendar.err_start_required');
        if (f.end_at) {
            // Compare like with like: an all-day form holds plain dates while
            // a timed one holds `YYYY-MM-DDTHH:mm`.
            const bothTimed = f.start_at.includes('T') && f.end_at.includes('T');
            const start = bothTimed ? f.start_at : f.start_at.split('T')[0];
            const end = bothTimed ? f.end_at : f.end_at.split('T')[0];
            if (end < start) return i18n.global.t('calendar.err_end_before_start');
        }
        const rule = f.recurrence;
        if (rule.freq !== 'none' && rule.endMode === 'until' && rule.until
            && rule.until < f.start_at.split('T')[0]) {
            return i18n.global.t('calendar.err_repeat_end_before_start');
        }
        if (rule.freq !== 'none' && (!Number.isFinite(rule.interval) || rule.interval < 1)) {
            return i18n.global.t('calendar.err_interval');
        }
        /*
         * There is deliberately no "a weekly rule must name a weekday" here.
         * `FREQ=WEEKLY` on its own is legal, means "weekly on the day it
         * starts", and is the commonest weekly rule in the wild — every ICS
         * import brings them in. Refusing it made those events impossible to
         * edit or drag at all, which is a strange way to treat data the app
         * itself just accepted. The editor seeds a weekday when somebody
         * picks "weekly" and will not let them remove the last one, so the
         * form never produces an empty list of its own.
         */
        return '';
    });

    const showErrors = ref(false);

    // --- Submit ---
    const submitEvent = async () => {
        if (isSubscribed(eventForm.value._originalEvent ?? {})) return;
        if (formError.value) { showErrors.value = true; return; }
        
        if (eventForm.value.isEdit && eventForm.value._originalEvent && isSeries(eventForm.value._originalEvent)) {
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
            let relPath = eventForm.value.id;
            let isCreatingNewNode = !eventForm.value.isEdit || !relPath;
            
            const properties: any = {
                is_all_day: eventForm.value.isAllDay,
                start_at: eventForm.value.start_at,
                end_at: eventForm.value.end_at,
                location: eventForm.value.location,
                tags: finalTags,
                tzid: eventForm.value.tzid || null,
                colour: eventForm.value.colour || null,
                ...recurrenceProperties(eventForm.value.recurrence),
                series_id: eventForm.value.series_id,
                exceptions: eventForm.value.exceptions,
                reminders: eventForm.value.reminders
            };
            
            if (eventForm.value.isEdit && eventForm.value._editScope === 'all' && eventForm.value._originalEvent) {
                const parentEv = eventForm.value._originalEvent;
                const rootId = parentEv.series_id || parentEv.id;
                // Parts of a series can sit outside the days on screen, so the
                // family is fetched rather than filtered out of the view.
                const familyEvents: EventMetadata[] = await ns.getEventSeries(rootId);
                const rootEv = familyEvents.find((e: EventMetadata) => e.id === rootId) || parentEv;
                
                const origStartDate = (rootEv.start_at || '').split('T')[0];
                const origEndDate = (rootEv.end_at || '').split('T')[0];

                // How far the user moved this instance, in whole local days.
                // Measured against the instance's own start rather than the
                // day cell that was clicked: on the second day of a multi-day
                // instance those differ, and the series would drift by the
                // difference without anyone touching the date.
                const shiftDays = daysBetween(
                    (parentEv.start_at || '').split('T')[0],
                    eventForm.value.start_at.split('T')[0],
                );

                const shiftedOrigStartDate = origStartDate
                    ? shiftDateString(origStartDate, shiftDays) : '';
                const shiftedOrigEndDate = origEndDate
                    ? shiftDateString(origEndDate, shiftDays) : shiftedOrigStartDate;
                
                const newTimeStart = eventForm.value.start_at.includes('T') ? eventForm.value.start_at.split('T')[1] : '';
                const newTimeEnd = eventForm.value.end_at.includes('T') ? eventForm.value.end_at.split('T')[1] : '';
                
                properties.start_at = newTimeStart ? `${shiftedOrigStartDate}T${newTimeStart}` : shiftedOrigStartDate;
                properties.end_at = newTimeEnd ? `${shiftedOrigEndDate}T${newTimeEnd}` : shiftedOrigEndDate;
                
                properties.series_id = '';

                // An `exceptions` entry means one of two different things, and
                // the model does not record which: either the user cancelled
                // that occurrence, or a split child took it over. Clearing the
                // whole array — what this used to do — resurrected every
                // cancelled occurrence the moment anyone edited the series.
                // Only the dates a child is about to vacate may be dropped.
                const overriddenDates = new Set(
                    familyEvents
                        .filter((f: EventMetadata) => f.id !== rootId)
                        .map((f: EventMetadata) => (f.start_at || '').split('T')[0])
                        .filter(Boolean),
                );
                properties.exceptions = (rootEv.exceptions || [])
                    .filter(d => !overriddenDates.has(d))
                    .map(d => (shiftDays ? shiftDateString(d, shiftDays) : d));
                // Splitting a series shortened the parent; merging it back has
                // to restore the reach the family had between them, unless the
                // user just set an end of their own.
                let maxEndAt = '';
                let isInfinite = false;
                for (const fam of familyEvents) {
                    const famRule = ruleOf(fam);
                    if (famRule.freq === 'none') continue;
                    if (famRule.endMode !== 'until' || !famRule.until) {
                        isInfinite = true;
                        break;
                    }
                    if (famRule.until > maxEndAt) maxEndAt = famRule.until;
                }
                const parentRule = ruleOf(parentEv);
                const endUntouched = eventForm.value.recurrence.endMode === parentRule.endMode
                    && eventForm.value.recurrence.until === parentRule.until;
                if (endUntouched) {
                    const merged = isInfinite || !maxEndAt
                        ? { ...eventForm.value.recurrence, endMode: 'never' as const, until: '' }
                        : endingOn(eventForm.value.recurrence, maxEndAt);
                    Object.assign(properties, recurrenceProperties(merged));
                }
                
                for (const famEv of familyEvents) {
                    if (famEv.id !== rootId) {
                        await ns.deleteNode({ relPath: famEv.id, silent: true });
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
                // A single moved occurrence stops repeating; a "and following"
                // split carries the rule on from where it was cut.
                Object.assign(properties, recurrenceProperties(
                    eventForm.value._editScope === 'this'
                        ? defaultRecurrence()
                        : eventForm.value.recurrence,
                ));
                properties.series_id = parentEv.series_id || parentEv.id;
                properties.exceptions = []; // New split event should not inherit exceptions
                const parentProps = {
                    is_all_day: parentEv.is_all_day,
                    start_at: parentEv.start_at,
                    end_at: parentEv.end_at,
                    location: parentEv.location,
                    tags: parentEv.tags,
                    ...recurrenceProperties(ruleOf(parentEv)),
                    exceptions: [...(parentEv.exceptions || [])],
                    relations: [...(parentEv.relations || [])],
                    series_id: parentEv.series_id
                };
                
                if (eventForm.value._editScope === 'this') {
                    if (!parentProps.exceptions.includes(targetOccurrenceDate.value)) {
                        parentProps.exceptions.push(targetOccurrenceDate.value);
                    }
                } else if (eventForm.value._editScope === 'following') {
                    // The parent now stops the day before the split.
                    Object.assign(parentProps, recurrenceProperties(
                        endingOn(ruleOf(parentEv), shiftDateString(targetOccurrenceDate.value, -1)),
                    ));
                }
                
                await ns.writeNode({
                    relPath: parentEv.id,
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

    /**
     * Move or resize one occurrence, from a drag rather than the form.
     *
     * It goes through the same write path as the dialog — including the
     * this / following / all question for a series — because a drag on the
     * third Monday of a stand-up is exactly as ambiguous as an edit of it.
     * The dialog itself never opens.
     */
    /**
     * How to put back the last event a drag moved.
     *
     * Only for the plain case: one event, not part of a series, moved to
     * another time. A series goes through the this / following / all question
     * and may have been split into a new node on the way — putting *that*
     * back is a different operation, and offering an undo that quietly does
     * something else is worse than not offering one.
     */
    const lastMove = ref<{ event: EventMetadata; startAt: string; endAt: string } | null>(null);

    const undoMove = async () => {
        const move = lastMove.value;
        if (!move) return false;
        lastMove.value = null;
        eventForm.value = formFromEvent(move.event, 'occurrence_view');
        eventForm.value.start_at = move.startAt;
        eventForm.value.end_at = move.endAt;
        eventForm.value._editScope = 'all';
        await submitEventActual();
        return true;
    };

    const rescheduleEvent = async (ev: EventMetadata, dateStr: string, startAt: string, endAt: string) => {
        // The grid does not offer to drag one of these, and this is the guard
        // for the day something changes and it does. A subscribed event has
        // no file to write to; the write would fail, but it would fail after
        // asking the user which occurrences they meant.
        if (isSubscribed(ev)) return;

        targetOccurrenceDate.value = dateStr;
        pendingEventAction.value = ev;
        eventForm.value = formFromEvent(ev, 'occurrence_view');

        // The grid works in the reader's zone. An event written somewhere else
        // has to go back into that zone before it is stored, or dropping it a
        // half hour later would also move it by the offset between them.
        const zone = (ev.tzid || '').trim();
        const here = localTimeZone();
        if (zone && here && zone !== here) {
            try {
                const [movedStart, movedEnd] = await ns.convertEventTime([startAt, endAt], here, zone);
                startAt = movedStart ?? startAt;
                endAt = movedEnd ?? endAt;
            } catch (e) {
                logger.error('Could not move the event back into its own zone:', e);
                return;
            }
        }

        eventForm.value.start_at = startAt;
        eventForm.value.end_at = endAt;

        if (formError.value) return;

        if (isSeries(ev)) {
            // A series was moved; the way back is not a single write.
            lastMove.value = null;
            scopeAction.value = 'edit';
            scopeSelection.value = 'this';
            showScopeModal.value = true;
            return;
        }

        lastMove.value = {
            event: ev,
            startAt: (ev.start_at || '').slice(0, 16),
            endAt: (ev.end_at || '').slice(0, 16),
        };
        eventForm.value._editScope = 'all';
        await submitEventActual();
    };

    // --- Delete ---
    const deleteEvent = async (ev: EventMetadata, dateStr: string) => {
        // Somebody else's calendar. Removing the subscription is how it goes.
        if (isSubscribed(ev)) return;
        if (isSeries(ev)) {
            scopeAction.value = 'delete';
            scopeSelection.value = 'this';
            targetOccurrenceDate.value = dateStr;
            pendingEventAction.value = ev;
            showScopeModal.value = true;
        } else {
            const isConfirmed = await ask(i18n.global.t('calendar.delete_event_body'), {
                title: i18n.global.t('calendar.delete_event_title', { title: ev.title }),
                kind: 'warning',
                okLabel: i18n.global.t('calendar.delete'),
                cancelLabel: i18n.global.t('calendar.cancel'),
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
                const familyEvents: EventMetadata[] = await ns.getEventSeries(rootId);
                for (const famEv of familyEvents) {
                    if (famEv.id !== ev.id) {
                        await ns.deleteNode({ relPath: famEv.id, silent: true });
                    }
                }
                await ns.deleteNode({ relPath: ev.id });
            } else {
                const parentProps = {
                    is_all_day: ev.is_all_day,
                    start_at: ev.start_at,
                    end_at: ev.end_at,
                    location: ev.location,
                    tags: ev.tags,
                    ...recurrenceProperties(ruleOf(ev)),
                    exceptions: [...(ev.exceptions || [])],
                    relations: [...(ev.relations || [])],
                    series_id: ev.series_id
                };
                
                if (scope === 'this') {
                    if (!parentProps.exceptions.includes(dateStr)) {
                        parentProps.exceptions.push(dateStr);
                    }
                } else if (scope === 'following') {
                    Object.assign(parentProps, recurrenceProperties(
                        endingOn(ruleOf(ev), shiftDateString(dateStr, -1)),
                    ));
                }
                
                await ns.writeNode({
                    relPath: ev.id,
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
    /**
     * Returns the write it starts. Callers that need to know when the vault
     * has caught up — anything reloading after it, and every test of this
     * path — have to be able to wait for it.
     */
    const confirmScopeAction = () => {
        showScopeModal.value = false;
        if (scopeAction.value === 'edit') {
            eventForm.value._editScope = scopeSelection.value as any;
            return submitEventActual();
        }
        return deleteEventActual(pendingEventAction.value!, targetOccurrenceDate.value, scopeSelection.value);
    };

    return {
        showEventForm, eventForm,
        showScopeModal, scopeAction, scopeSelection, targetOccurrenceDate, pendingEventAction,
        startAtDate, startAtHour, startAtMinute, startAtMinuteOptions,
        endAtDate, endAtHour, endAtMinute, endAtMinuteOptions,
        reminderPreset, customReminder, reminderError, addReminder, removeReminder,
        formError, showErrors,
        openAddEventModal, openEditEventModal, closeEventForm, rescheduleEvent,
        lastMove, undoMove,
        submitEvent, deleteEvent, handleDeleteFromForm, confirmScopeAction,
    };
}
