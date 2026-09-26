/**
 * What a routine looks like to the screen, and how its schedule is said.
 *
 * Mirrors `syn::routine`. The backend decides when a routine runs; this only
 * says it back in words, so "1,2,3,4,5 at 07:30" reads as "Mon–Fri at 07:30".
 */
export interface Routine {
  id: string;
  name: string;
  ask: string;
  schedule: { at: string; weekdays: number[] };
  enabled: boolean;
  to_phone: boolean;
  conversation_id?: string | null;
}

export interface RoutineView extends Routine {
  /** Local `YYYY-MM-DDTHH:MM`, or null when off. */
  next_run: string | null;
  last_slot: string | null;
}

export const blankRoutine = (): Routine => ({
  id: '',
  name: '',
  ask: '',
  schedule: { at: '07:30', weekdays: [] },
  enabled: true,
  to_phone: false,
});

/**
 * The days, in words. Every day and weekdays are said as such; anything else
 * is listed, in week order, using the locale's short names.
 */
export function daysInWords(weekdays: number[], t: (key: string) => string): string {
  const days = [...new Set(weekdays)].filter(d => d >= 1 && d <= 7).sort((a, b) => a - b);
  if (days.length === 0 || days.length === 7) return t('syn.routine_every_day');
  if (days.join() === '1,2,3,4,5') return `${t('syn.weekday_1')}–${t('syn.weekday_5')}`;
  return days.map(d => t(`syn.weekday_${d}`)).join(', ');
}

/** "2026-09-28T07:30" as the person's locale says a day and a time. */
export function whenInWords(slot: string | null, locale?: string): string {
  if (!slot) return '';
  const at = new Date(slot);
  if (Number.isNaN(at.getTime())) return slot;
  return at.toLocaleString(locale, { weekday: 'short', day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
}

/** Add a day if it is not there, take it away if it is. */
export function toggleDay(weekdays: number[], day: number): number[] {
  return weekdays.includes(day) ? weekdays.filter(d => d !== day) : [...weekdays, day].sort((a, b) => a - b);
}
