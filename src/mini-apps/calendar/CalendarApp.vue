<script setup lang="ts">
import { toRef } from 'vue';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import type { ViewMode, EventMetadata } from './types';

// ── Components ──────────────────────────────────────────────
import CalendarHeader from './components/CalendarHeader.vue';
import MonthView from './components/MonthView.vue';
import DayView from './components/DayView.vue';
import WeekView from './components/WeekView.vue';
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

// ── Props & Services ────────────────────────────────────────
const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open-node', id: string, type: string): void }>();
const bus = useEventBus();
const ns = useNodeService();
const vaultPathRef = toRef(props, 'vaultPath');

// ── Composable Wiring ───────────────────────────────────────
const data = useCalendarData(ns, bus, vaultPathRef);
const helpers = useCalendarHelpers(data.allTasks, data.allEvents);
const nav = useCalendarNavigation(
    helpers.getTasksForDate, helpers.getEventsForDate, helpers.hasItemsOnDate,
);

const form = useEventForm(
    data.allEvents, ns,
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
         />

         <div class="flex-1 min-h-0 relative w-full">
             <MonthView v-show="nav.viewMode.value === 'month'"
                 :calendar-days="nav.calendarDays.value"
                 :selected-date="nav.selectedDate.value"
                 :get-events-for-date="helpers.getEventsForDate"
                 :get-tasks-for-date="helpers.getTasksForDate"
                 :get-month-view-items="helpers.getMonthViewItems"
                 @click-day="nav.clickDay"
                 @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
                 @toggle-task="data.toggleTaskStatus"
                 @open-task="handleOpenTask"
             />

             <DayView v-if="nav.viewMode.value === 'day'"
                 :current-date="nav.currentDate.value"
                 :get-tasks-for-date="helpers.getTasksForDate"
                 :get-events-for-date="helpers.getEventsForDate"
                 :get-events-for-date-and-hour="helpers.getEventsForDateAndHour"
                 @click-day="nav.clickDay"
                 @add-event="(d: Date, hr?: number) => form.openAddEventModal(d, hr)"
                 @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
                 @toggle-task="data.toggleTaskStatus"
                 @open-task="handleOpenTask"
             />

             <WeekView v-if="nav.viewMode.value === 'week'"
                 :current-week-days="nav.currentWeekDays.value"
                 :get-tasks-for-date="helpers.getTasksForDate"
                 :get-events-for-date="helpers.getEventsForDate"
                 :get-events-for-date-and-hour="helpers.getEventsForDateAndHour"
                 @click-day="nav.clickDay"
                 @add-event="(d: Date, hr?: number) => form.openAddEventModal(d, hr)"
                 @edit-event="(ev: EventMetadata, ds: string) => form.openEditEventModal(ev, ds)"
                 @toggle-task="data.toggleTaskStatus"
                 @open-task="handleOpenTask"
             />

             <YearView v-if="nav.viewMode.value === 'year'"
                 :year-months="nav.yearMonths.value"
                 :current-date="nav.currentDate.value"
                 @click-year-day="nav.clickYearDay"
                 @go-to-month="handleGoToMonth"
             />
         </div>
     </div>

     <DayDetailPanel
         :show="nav.showRightPanel.value"
         :selected-date-display="nav.selectedDateDisplay.value"
         :selected-events="nav.selectedEvents.value"
         :selected-tasks="nav.selectedTasks.value"
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
         :event-relations="relations.eventRelations.value"
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
     />
  </div>
</template>
