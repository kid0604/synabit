import { ref, computed } from 'vue';
import type { ViewMode, TaskMetadata, EventMetadata } from '../types';
import { monthNames, monthNamesShort, formatDateString, isSameDay } from '../helpers';

export function useCalendarNavigation(
    getTasksForDate: (dateStr: string) => TaskMetadata[],
    getEventsForDate: (dateStr: string) => EventMetadata[],
    hasItemsOnDate: (date: Date) => boolean,
) {
    const viewMode = ref<ViewMode>('month');
    const currentDate = ref(new Date());
    const selectedDate = ref<Date>(new Date());
    const showRightPanel = ref(false);

    const headerDisplayString = computed(() => {
        const year = currentDate.value.getFullYear();
        if (viewMode.value === 'year') return `${year}`;
        if (viewMode.value === 'month') return `${monthNames[currentDate.value.getMonth()]} ${year}`;
        if (viewMode.value === 'day') return `${currentDate.value.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric'})}, ${year}`;
        if (viewMode.value === 'week') {
            const week = currentWeekDays.value;
            const first = week[0].date;
            const last = week[6].date;
            if (first.getMonth() === last.getMonth()) {
                return `${monthNames[first.getMonth()]} ${year}`;
            } else if (first.getFullYear() === last.getFullYear()) {
                return `${monthNamesShort[first.getMonth()]} - ${monthNamesShort[last.getMonth()]} ${year}`;
            } else {
                return `${monthNamesShort[first.getMonth()]} ${first.getFullYear()} - ${monthNamesShort[last.getMonth()]} ${last.getFullYear()}`;
            }
        }
        return '';
    });

    const navigatePrev = () => {
        const d = new Date(currentDate.value);
        if (viewMode.value === 'month') d.setMonth(d.getMonth() - 1);
        else if (viewMode.value === 'day') d.setDate(d.getDate() - 1);
        else if (viewMode.value === 'week') d.setDate(d.getDate() - 7);
        else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() - 1);
        currentDate.value = d;
    };

    const navigateNext = () => {
        const d = new Date(currentDate.value);
        if (viewMode.value === 'month') d.setMonth(d.getMonth() + 1);
        else if (viewMode.value === 'day') d.setDate(d.getDate() + 1);
        else if (viewMode.value === 'week') d.setDate(d.getDate() + 7);
        else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() + 1);
        currentDate.value = d;
    };

    const goToToday = () => {
        currentDate.value = new Date();
        selectedDate.value = new Date();
        if (viewMode.value === 'year') viewMode.value = 'month'; // Jump to month mode if today clicked from year
        showRightPanel.value = false;
    };

    // 1. Month Mode
    const calendarDays = computed(() => {
        const year = currentDate.value.getFullYear();
        const month = currentDate.value.getMonth();
        const firstDay = new Date(year, month, 1);
        const startDayOfWeek = firstDay.getDay();
        const prevMonthDays = new Date(year, month, 0).getDate();
        const lastDayOfMonth = new Date(year, month + 1, 0).getDate();
        
        const days = [];
        for (let i = startDayOfWeek - 1; i >= 0; i--) {
            days.push({ date: new Date(year, month - 1, prevMonthDays - i), inMonth: false });
        }
        for (let d = 1; d <= lastDayOfMonth; d++) {
            days.push({ date: new Date(year, month, d), inMonth: true });
        }
        let nextI = 1;
        while (days.length % 7 !== 0 || days.length < 42) {
            days.push({ date: new Date(year, month + 1, nextI++), inMonth: false });
        }
        return days;
    });

    // 2. Week Mode
    const currentWeekDays = computed(() => {
        const d = new Date(currentDate.value);
        const day = d.getDay();
        const diff = d.getDate() - day; // Sunday is 0
        const startOfWeek = new Date(d.setDate(diff));
        const week = [];
        for (let i = 0; i < 7; i++) {
            const cur = new Date(startOfWeek);
            cur.setDate(startOfWeek.getDate() + i);
            week.push({ date: cur, dateStr: formatDateString(cur) });
        }
        return week;
    });

    // 3. Year Mode
    const yearMonths = computed(() => {
        const year = currentDate.value.getFullYear();
        return Array.from({length: 12}, (_, i) => { // i is month index (0-11)
            const daysInMonth = new Date(year, i + 1, 0).getDate();
            const startDayOfWeek = new Date(year, i, 1).getDay();
            const days = [];
            // empty paddings
            for (let p=0; p<startDayOfWeek; p++) days.push(null);
            // real days
            for (let d=1; d<=daysInMonth; d++) {
                const dt = new Date(year, i, d);
                days.push({
                    date: dt,
                    hasItems: hasItemsOnDate(dt),
                    isToday: isSameDay(dt, new Date())
                });
            }
            return { monthIndex: i, name: monthNames[i], days };
        });
    });

    const clickDay = (dateObj: Date) => {
        selectedDate.value = dateObj;
        // Auto-update currentDate to follow the selection into views
        currentDate.value = new Date(dateObj);
        if (viewMode.value !== 'day' && viewMode.value !== 'week') {
            showRightPanel.value = true;
        }
    };

    const clickYearDay = (dt: Date) => {
        selectedDate.value = dt;
        currentDate.value = new Date(dt);
        viewMode.value = 'day';
        showRightPanel.value = false;
    };

    // Panel computeds
    const selectedDateFormattedStr = computed(() => formatDateString(selectedDate.value));
    const selectedDateDisplay = computed(() => selectedDate.value.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }));
    const selectedTasks = computed(() => getTasksForDate(selectedDateFormattedStr.value));
    const selectedEvents = computed(() => getEventsForDate(selectedDateFormattedStr.value).sort((a,b) => (a.start_at || '').localeCompare(b.start_at || '')));

    return {
        viewMode, currentDate, selectedDate, showRightPanel,
        headerDisplayString, navigatePrev, navigateNext, goToToday,
        calendarDays, currentWeekDays, yearMonths,
        clickDay, clickYearDay,
        selectedDateFormattedStr, selectedDateDisplay, selectedTasks, selectedEvents,
    };
}
