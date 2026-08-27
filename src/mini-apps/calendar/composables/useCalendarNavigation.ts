import { ref, computed } from 'vue';
import type { ViewMode } from '../types';
import { monthName, monthNameShort, formatDateString, isSameDay, weekdayOffset } from '../helpers';
import { i18n } from '../../../i18n';

/** The app's language, not the operating system's. */
const appLocale = () => i18n.global.locale.value;

/**
 * Where the calendar is pointed, and — because the two cannot be separated —
 * which days it is therefore asking the vault about.
 *
 * It no longer takes the data lookups it used to. Data now depends on the
 * visible range, so navigation cannot depend on data without a cycle: the
 * lookups that used to live here are computed in `CalendarApp` and handed to
 * the views that need them, the same way `MonthView` already receives them.
 */
export function useCalendarNavigation() {
    const viewMode = ref<ViewMode>('month');
    const currentDate = ref(new Date());
    const selectedDate = ref<Date>(new Date());
    const showRightPanel = ref(false);

    const headerDisplayString = computed(() => {
        const year = currentDate.value.getFullYear();
        // The agenda is not a page of anything, so it has no period to name.
        if (viewMode.value === 'agenda') return '';
        if (viewMode.value === 'year') return `${year}`;
        if (viewMode.value === 'month') return `${monthName(currentDate.value.getMonth())} ${year}`;
        if (viewMode.value === 'day') return `${currentDate.value.toLocaleDateString(appLocale(), { weekday: 'long', month: 'long', day: 'numeric'})}, ${year}`;
        if (viewMode.value === 'week') {
            const week = currentWeekDays.value;
            const first = week[0].date;
            const last = week[6].date;
            if (first.getMonth() === last.getMonth()) {
                return `${monthName(first.getMonth())} ${year}`;
            } else if (first.getFullYear() === last.getFullYear()) {
                return `${monthNameShort(first.getMonth())} - ${monthNameShort(last.getMonth())} ${year}`;
            } else {
                return `${monthNameShort(first.getMonth())} ${first.getFullYear()} - ${monthNameShort(last.getMonth())} ${last.getFullYear()}`;
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
        // Not `getDay()`: the grid's first column is whatever day this locale
        // starts its week on, which for Vietnamese is Monday.
        const startDayOfWeek = weekdayOffset(firstDay);
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
        const startOfWeek = new Date(d);
        startOfWeek.setDate(d.getDate() - weekdayOffset(d));
        const week = [];
        for (let i = 0; i < 7; i++) {
            const cur = new Date(startOfWeek);
            cur.setDate(startOfWeek.getDate() + i);
            week.push({ date: cur, dateStr: formatDateString(cur) });
        }
        return week;
    });

    // 2b. Day Mode — the same shape as a week, one column wide.
    const currentDayColumn = computed(() => [{
        date: currentDate.value,
        dateStr: formatDateString(currentDate.value),
    }]);

    // 3. Year Mode — the shape of the year, with no opinion about its contents.
    const yearMonths = computed(() => {
        const year = currentDate.value.getFullYear();
        return Array.from({length: 12}, (_, i) => { // i is month index (0-11)
            const daysInMonth = new Date(year, i + 1, 0).getDate();
            const startDayOfWeek = weekdayOffset(new Date(year, i, 1));
            const days = [];
            // empty paddings
            for (let p=0; p<startDayOfWeek; p++) days.push(null);
            // real days
            for (let d=1; d<=daysInMonth; d++) {
                const dt = new Date(year, i, d);
                days.push({ date: dt, isToday: isSameDay(dt, new Date()) });
            }
            return { monthIndex: i, name: monthName(i), days };
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
    const selectedDateDisplay = computed(() => selectedDate.value.toLocaleDateString(appLocale(), { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }));

    /**
     * The days on screen, which is exactly what the vault is asked for.
     *
     * The selected day is folded in because the detail panel can outlive a
     * change of view, and a panel listing a day nobody fetched would simply
     * look empty rather than wrong.
     */
    const visibleRange = computed<{ from: string; to: string }>(() => {
        let first: Date;
        let last: Date;
        if (viewMode.value === 'year') {
            const y = currentDate.value.getFullYear();
            first = new Date(y, 0, 1);
            last = new Date(y, 11, 31);
        } else if (viewMode.value === 'month') {
            const days = calendarDays.value;
            first = days[0].date;
            last = days[days.length - 1].date;
        } else if (viewMode.value === 'week') {
            const week = currentWeekDays.value;
            first = week[0].date;
            last = week[6].date;
        } else {
            first = currentDate.value;
            last = currentDate.value;
        }
        const from = formatDateString(first);
        const to = formatDateString(last);
        const sel = selectedDateFormattedStr.value;
        return { from: sel < from ? sel : from, to: sel > to ? sel : to };
    });

    return {
        viewMode, currentDate, selectedDate, showRightPanel,
        headerDisplayString, navigatePrev, navigateNext, goToToday,
        calendarDays, currentWeekDays, currentDayColumn, yearMonths,
        clickDay, clickYearDay,
        selectedDateFormattedStr, selectedDateDisplay,
        visibleRange,
    };
}
