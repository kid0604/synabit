<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, toRef, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { logger } from '../../utils/logger';
import { useNodeService } from '../../composables/useNodeService';
import type { ViewMode, EventMetadata } from './types';

// ── Components ──────────────────────────────────────────────
import CalendarHeader from './components/CalendarHeader.vue';
import MonthView from './components/MonthView.vue';
import TimeGridView from './components/TimeGridView.vue';
import YearView from './components/YearView.vue';
import DayDetailPanel from './components/DayDetailPanel.vue';
import ScopeModal from './components/ScopeModal.vue';
import EventFormModal from './components/EventFormModal.vue';

// ── Composables ─────────────────────────────────────────────
import { useCalendarData } from './composables/useCalendarData';
import { useCalendarHelpers } from './composables/useCalendarHelpers';
import { useCalendarNavigation } from './composables/useCalendarNavigation';
import { useEventForm } from './composables/useEventForm';
import { useEventRelations } from './composables/useEventRelations';
import { useCalendarExchange } from './composables/useCalendarExchange';
import { useSubscriptions } from './composables/useSubscriptions';
import { isSubscribed } from './subscriptions';
import SubscriptionsPanel from './components/SubscriptionsPanel.vue';
import AgendaView from './components/AgendaView.vue';
import { useAgenda } from './composables/useAgenda';

// ── Props & Services ────────────────────────────────────────
const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open-node', id: string, type: string): void }>();
const bus = useEventBus();
const ns = useNodeService();
const vaultPathRef = toRef(props, 'vaultPath');

// ── Composable Wiring ───────────────────────────────────────
// Order matters, and reads the way the data flows: navigation decides which
// days are on screen, the vault is asked about exactly those days, and the
// lookups read the answer. Navigation used to take the lookups as arguments,
// which is why it could not also own the range.
const nav = useCalendarNavigation();
const data = useCalendarData(ns, bus, vaultPathRef, nav.visibleRange);
const helpers = useCalendarHelpers(data.tasksByDate, data.eventsByDate);

const selectedTasks = computed(() => helpers.getTasksForDate(nav.selectedDateFormattedStr.value));
const selectedEvents = computed(() => helpers.getSortedEventsForDate(nav.selectedDateFormattedStr.value));

const form = useEventForm(
    ns,
    nav.selectedDateFormattedStr,
    data.loadData,
    async (title: string, id: string) => { await relations.loadEventBacklinks(title, id); },
    () => { relations.resetEventBacklinks(); },
    () => { relations.resetCreatingNote(); },
);

const relations = useEventRelations(
    ns, form.eventForm,
    () => { form.closeEventForm(); },
    emit as any,
);

/** Turn the boxes somebody ticked into a meeting note into real tasks. */
const makeTasksFromNotes = async (chosen: number[]) => {
    try {
        const made = await relations.makeTasksFromNotes(chosen);
        if (made > 0) {
            say(t('calendar.note_actions_made', { n: made }));
            await relations.loadNoteActions();
            await data.loadData();
        }
    } catch (e) {
        logger.error('Could not turn those notes into tasks:', e);
        say(t('calendar.exchange_failed'));
    }
};

/**
 * A drag writes straight away, so the way back is an offer rather than a
 * confirmation. Only for a plain move — a series goes through the
 * this/following/all question and may have been split, which is not one
 * write to reverse.
 */
const handleReschedule = async (ev: EventMetadata, dateStr: string, startAt: string, endAt: string) => {
    await form.rescheduleEvent(ev, dateStr, startAt, endAt);
    if (!form.lastMove.value) return;
    say(t('calendar.moved', { title: ev.title }), async () => {
        if (await form.undoMove()) {
            await data.loadEvents();
            say(t('calendar.undone'));
        }
    });
};

// ── Getting around without a mouse ──────────────────────────
/**
 * The shortcuts every calendar has, and this one did not.
 *
 * Ignored while somebody is typing, and while a dialog is open — a modal
 * takes the whole keyboard, and `d` in a title field is the letter d.
 */
const isTyping = (target: EventTarget | null) => {
    const el = target as HTMLElement | null;
    if (!el) return false;
    const tag = el.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el.isContentEditable;
};

const onShortcut = (e: KeyboardEvent) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (isTyping(e.target)) return;
    if (form.showEventForm.value || form.showScopeModal.value || showSubscriptions.value) return;

    const views: Record<string, ViewMode> = {
        d: 'day', w: 'week', m: 'month', y: 'year', a: 'agenda',
    };
    const key = e.key.toLowerCase();

    if (key in views) { nav.viewMode.value = views[key]; e.preventDefault(); return; }
    if (key === 't') { nav.goToToday(); e.preventDefault(); return; }
    if (key === 'n') { form.openAddEventModal(); e.preventDefault(); return; }
    // The grid's own arrow keys move between days; these turn the page, so
    // they only apply when the grid does not have focus.
    if (e.key === 'ArrowLeft' && !isInGrid(e.target)) { nav.navigatePrev(); e.preventDefault(); return; }
    if (e.key === 'ArrowRight' && !isInGrid(e.target)) { nav.navigateNext(); e.preventDefault(); }
};

const isInGrid = (target: EventTarget | null) =>
    !!(target as HTMLElement | null)?.closest?.('[role="grid"]');

onMounted(() => { window.addEventListener('keydown', onShortcut); });
onUnmounted(() => { window.removeEventListener('keydown', onShortcut); });

// ── Deciding when to do something ───────────────────────────
/**
 * A task dragged onto an hour becomes an event that points back at it.
 *
 * The task is not moved and not changed. Two things that mean different
 * things — "this needs doing" and "I am doing it then" — stay two things, and
 * the link is what keeps them the same subject. Ticking the task off still
 * ticks the task off.
 */
const blockTask = async (
    task: { id: string; title: string },
    dateStr: string,
    startAt: string,
    endAt: string,
) => {
    try {
        const relPath = `Events/${crypto.randomUUID()}.md`;
        await ns.writeNode({
            relPath,
            title: task.title,
            nodeType: 'event',
            properties: {
                is_all_day: false,
                start_at: startAt,
                end_at: endAt,
                relations: [`[${task.title}](synabit://task/${task.id})`],
            },
            content: '',
            eventType: 'created',
        });
        await data.loadEvents();
        say(t('calendar.blocked_task', { title: task.title }), async () => {
            await ns.deleteNode({ relPath, silent: true });
            await data.loadEvents();
            say(t('calendar.undone'));
        });
    } catch (e) {
        logger.error('Could not schedule that task:', e);
        say(t('calendar.exchange_failed'));
    }
};

// ── Finding a meeting rather than looking at a week ─────────
const agenda = useAgenda();

/**
 * The agenda reads the vault for itself, over its own range — a search looks
 * a year back, which is nothing like the days the grid is showing. It reloads
 * when it is opened and whenever anything changes underneath it.
 */
watch(() => nav.viewMode.value, (mode) => {
    if (mode === 'agenda') agenda.load();
});

/** Every meeting with one person — the question the grid cannot answer. */
const showMeetingsWith = (id: string, name: string) => {
    form.closeEventForm();
    agenda.focusOn(id, name);
    nav.viewMode.value = 'agenda';
    nav.showRightPanel.value = false;
};

// ── Calendars belonging to somebody else ────────────────────
const subs = useSubscriptions(() => {
    data.loadEvents();
    if (nav.viewMode.value === 'agenda') agenda.load();
});
const showSubscriptions = ref(false);

const addSubscription = async (url: string, name: string) => {
    try {
        const report = await subs.add(url, name);
        if (!report) return;
        say(report.error
            ? report.error
            : t('calendar.subscribe_added', { name: report.name, n: report.events }));
    } catch (e) {
        logger.error('Could not subscribe to that calendar:', e);
        say(t('calendar.subscribe_failed'));
    }
};

const refreshSubscriptions = async () => {
    try {
        const reports = await subs.refreshAll();
        const failed = reports.filter(r => r.error);
        say(failed.length
            ? failed[0].error
            : t('calendar.subscribe_refreshed', { n: reports.length }));
    } catch (e) {
        logger.error('Could not refresh the subscribed calendars:', e);
        say(t('calendar.subscribe_failed'));
    }
};

/** The calendar an event came from, for the read-only banner. */
const sourceOf = (id: string | undefined) =>
    subs.subscriptions.value.find(s => s.id === id)?.name ?? '';

// ── Taking the calendar out, and bringing one in ────────────
const { t } = useI18n();
const exchange = useCalendarExchange(ns);
const notice = ref('');
/** Shown beside the notice while the last thing done can still be taken back. */
const undoAction = ref<null | (() => Promise<void>)>(null);

const say = (message: string, undo?: () => Promise<void>) => {
    notice.value = message;
    undoAction.value = undo ?? null;
    setTimeout(() => {
        if (notice.value !== message) return;
        notice.value = '';
        undoAction.value = null;
    }, undo ? 8000 : 4000);
};

const takeItBack = async () => {
    const undo = undoAction.value;
    if (!undo) return;
    notice.value = '';
    undoAction.value = null;
    try {
        await undo();
    } catch (e) {
        logger.error('Could not undo that:', e);
        say(t('calendar.exchange_failed'));
    }
};

const handleExport = async () => {
    try {
        const count = await exchange.exportIcs();
        // Null means the file dialog was closed, which is not a failure and
        // must not be reported as one.
        if (count !== null) say(t('calendar.exported_n', { n: count }));
    } catch (e) {
        logger.error('Could not export the calendar:', e);
        say(t('calendar.exchange_failed'));
    }
};

const handleImport = async () => {
    try {
        const summary = await exchange.importIcs();
        if (!summary) return;
        if (summary.added === 0 && summary.updated === 0) {
            say(t('calendar.imported_none'));
        } else {
            // An event that lost its repeat is worth saying out loud: it is
            // on the right day, and it will not come back next month.
            const notes = [
                summary.skipped ? t('calendar.import_skipped', { n: summary.skipped }) : '',
                summary.lostRepeat ? t('calendar.import_lost_repeat', { n: summary.lostRepeat }) : '',
            ].filter(Boolean).join(' ');
            // The two counts the message actually interpolates, rather than the
            // whole summary: an interface has no index signature, so it is not
            // assignable to the named-values parameter.
            say([
                t('calendar.imported_n', { added: summary.added, updated: summary.updated }),
                notes,
            ].filter(Boolean).join(' '));
            await data.loadData();
        }
    } catch (e) {
        logger.error('Could not import that calendar:', e);
        say(t('calendar.exchange_failed'));
    }
};

// ── Event Handlers ──────────────────────────────────────────
const handleOpenTask = (id: string) => emit('open-node', id, 'task');

const handleGoToMonth = (monthIndex: number) => {
    nav.currentDate.value = new Date(nav.currentDate.value.getFullYear(), monthIndex, 1);
    nav.viewMode.value = 'month';
};
</script>

<template>
  <div class="h-full flex relative text-[#1c1c1e] dark:text-[#f4f4f5] bg-[#fdfdfc] dark:bg-[#242424]">
     <div class="flex-1 flex flex-col h-full overflow-hidden px-3 py-3 md:px-6 md:py-4 transition-all duration-300" :class="{ 'md:pr-96': nav.showRightPanel.value }">

         <CalendarHeader
             :header-display-string="nav.headerDisplayString.value"
             :view-mode="nav.viewMode.value"
             @update:view-mode="(v: ViewMode) => { nav.viewMode.value = v; if (v === 'day' || v === 'week') nav.showRightPanel.value = false; }"
             @navigate-prev="nav.navigatePrev"
             @navigate-next="nav.navigateNext"
             @go-today="nav.goToToday"
             @add-event="form.openAddEventModal()"
             @export-ics="handleExport"
             @import-ics="handleImport"
             @subscriptions="showSubscriptions = true"
         />

         <div class="flex-1 min-h-0 relative w-full">
             <MonthView v-show="nav.viewMode.value === 'month'"
                 :calendar-days="nav.calendarDays.value"
                 :selected-date="nav.selectedDate.value"
                 :get-events-for-date="helpers.getEventsForDate"
                 :get-tasks-for-date="helpers.getTasksForDate"
                 :get-month-view-items="helpers.getMonthViewItems"
                 @click-day="nav.clickDay"
                 @focus-day="(d: Date) => { nav.selectedDate.value = d; }"
                 @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
                 @toggle-task="data.toggleTaskStatus"
                 @open-task="handleOpenTask"
             />

             <!-- Day and week are the same grid; the only difference is how
                  many columns it has and whether they are labelled. -->
             <TimeGridView v-if="nav.viewMode.value === 'day' || nav.viewMode.value === 'week'"
                 :key="nav.viewMode.value"
                 :days="nav.viewMode.value === 'week' ? nav.currentWeekDays.value : nav.currentDayColumn.value"
                 :get-tasks-for-date="helpers.getTasksForDate"
                 :get-events-for-date="helpers.getEventsForDate"
                 :show-day-headers="nav.viewMode.value === 'week'"
                 :subscription-colours="subs.colours.value"
                 @click-day="nav.clickDay"
                 @add-event="(d: Date, s?: string, e?: string) => form.openAddEventModal(d, undefined, s, e)"
                 @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
                 @reschedule="handleReschedule"
                 @block-task="blockTask"
                 @toggle-task="data.toggleTaskStatus"
                 @open-task="handleOpenTask"
             />

             <AgendaView v-if="nav.viewMode.value === 'agenda'"
                 :days="agenda.days.value"
                 :loading="agenda.loading.value"
                 :query="agenda.query.value"
                 :person-name="agenda.personName.value"
                 :narrowed="agenda.narrowed.value"
                 :subscription-colours="subs.colours.value"
                 @update:query="(v: string) => agenda.query.value = v"
                 @clear-person="agenda.clearPerson"
                 @open-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
             />

             <YearView v-if="nav.viewMode.value === 'year'"
                 :year-months="nav.yearMonths.value"
                 :current-date="nav.currentDate.value"
                 :has-items-on-date="helpers.hasItemsOnDate"
                 @click-year-day="nav.clickYearDay"
                 @go-to-month="handleGoToMonth"
             />
         </div>
     </div>

     <DayDetailPanel
         :show="nav.showRightPanel.value"
         :selected-date-display="nav.selectedDateDisplay.value"
         :selected-events="selectedEvents"
         :selected-tasks="selectedTasks"
         :selected-date-formatted-str="nav.selectedDateFormattedStr.value"
         @close="nav.showRightPanel.value = false"
         @add-event="form.openAddEventModal()"
         @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
         @delete-event="(ev: EventMetadata, ds: string) => form.deleteEvent(ev, ds)"
         @toggle-task="data.toggleTaskStatus"
         @open-task="handleOpenTask"
     />

     <ScopeModal
         :show="form.showScopeModal.value"
         :action="form.scopeAction.value"
         :model-value="form.scopeSelection.value"
         @update:model-value="(v: any) => form.scopeSelection.value = v"
         @confirm="form.confirmScopeAction"
         @cancel="form.showScopeModal.value = false"
     />

     <EventFormModal
         :show="form.showEventForm.value"
         :form="form.eventForm.value"
         :start-at-date="form.startAtDate.value"
         :start-at-hour="form.startAtHour.value"
         :start-at-minute="form.startAtMinute.value"
         :start-at-minute-options="form.startAtMinuteOptions.value"
         :end-at-date="form.endAtDate.value"
         :end-at-hour="form.endAtHour.value"
         :end-at-minute="form.endAtMinute.value"
         :end-at-minute-options="form.endAtMinuteOptions.value"
         :reminder-preset="form.reminderPreset.value"
         :custom-reminder="form.customReminder.value"
         :reminder-error="form.reminderError.value"
         :form-error="form.formError.value"
         :show-errors="form.showErrors.value"
         :read-only="isSubscribed(form.eventForm.value._originalEvent ?? {})"
         :source-name="sourceOf(form.eventForm.value._originalEvent?.subscription_id)"
         :event-relations="relations.eventRelations.value"
         :event-people="relations.eventPeople.value"
         :people-query="relations.peopleQuery.value"
         :people-matches="relations.peopleMatches.value"
         :is-adding-person="relations.isAddingPerson.value"
         :note-actions="relations.noteActions.value"
         :is-making-tasks="relations.isMakingTasks.value"
         :is-creating-note="relations.isCreatingNote.value"
         :new-note-title="relations.newNoteTitle.value"
         @close="form.closeEventForm"
         @submit="form.submitEvent"
         @delete="form.handleDeleteFromForm"
         @update:start-at-date="(v: string) => form.startAtDate.value = v"
         @update:start-at-hour="(v: string) => form.startAtHour.value = v"
         @update:start-at-minute="(v: string) => form.startAtMinute.value = v"
         @update:end-at-date="(v: string) => form.endAtDate.value = v"
         @update:end-at-hour="(v: string) => form.endAtHour.value = v"
         @update:end-at-minute="(v: string) => form.endAtMinute.value = v"
         @update:reminder-preset="(v: string) => form.reminderPreset.value = v"
         @update:custom-reminder="(v: string) => form.customReminder.value = v"
         @add-reminder="form.addReminder"
         @remove-reminder="(idx: number) => form.removeReminder(idx)"
         @update:is-creating-note="(v: boolean) => relations.isCreatingNote.value = v"
         @update:new-note-title="(v: string) => relations.newNoteTitle.value = v"
         @create-note="relations.createMeetingNote"
         @delete-relation="relations.deleteRelationNode"
         @open-linked-note="(id: string, type: string) => relations.openLinkedNote(id, type)"
         @update:people-query="(v: string) => relations.peopleQuery.value = v"
         @update:is-adding-person="(v: boolean) => relations.isAddingPerson.value = v"
         @search-people="relations.searchPeople"
         @add-person="relations.addPerson"
         @remove-person="relations.removePerson"
         @load-note-actions="relations.loadNoteActions"
         @make-tasks="makeTasksFromNotes"
         @see-meetings-with="showMeetingsWith"
     />
     <SubscriptionsPanel
         :show="showSubscriptions"
         :subscriptions="subs.subscriptions.value"
         :busy="subs.busy.value"
         @close="showSubscriptions = false"
         @add="addSubscription"
         @remove="(id: string) => subs.remove(id)"
         @toggle="(id: string, on: boolean) => subs.setEnabled(id, on)"
         @toggle-remind="(id: string, on: boolean) => subs.setRemind(id, on)"
         @refresh="refreshSubscriptions"
     />

     <p v-if="notice" role="status"
        class="fixed bottom-8 left-1/2 -translate-x-1/2 bg-gray-900 dark:bg-white text-white dark:text-gray-900 px-5 py-3 rounded-xl shadow-xl z-[100] text-sm font-semibold max-w-md w-max flex items-center gap-3">
         <span>{{ notice }}</span>
         <button v-if="undoAction" type="button" @click="takeItBack"
                 class="shrink-0 underline underline-offset-2 font-bold hover:opacity-80 transition-opacity">
             {{ $t('calendar.undo') }}
         </button>
     </p>
  </div>
</template>
