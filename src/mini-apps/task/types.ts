// ── Task Types & Constants ──────────────────────────────────────────

export interface TaskMetadata {
  id: string;
  title: string;
  status: string;
  is_transferred: boolean;
  transferred_to: string;
  track_progress: boolean;
  priority: string;
  start_date: string;
  due_date: string;
  comment: string;
  source_link: string;
  tags: string[];
  content: string;
  path: string;
  created_at: string;
  updated_at: string;
  completed_at: string;
  project_id: string;
  custom_fields: Record<string, any>;
  isNew?: boolean;
}

export const BOARD_COLUMNS = [
  { id: 'backlog', name: 'BACKLOG', class: 'border-t-2 border-gray-400 dark:border-gray-500' },
  { id: 'todo', name: 'TO DO', class: 'border-t-2 border-gray-300 dark:border-gray-600' },
  { id: 'in_progress', name: 'IN PROGRESS', class: 'border-t-2 border-blue-400 dark:border-blue-500' },
  { id: 'done', name: 'DONE', class: 'border-t-2 border-green-400 dark:border-green-500' },
] as const;

export const URGENCY_THRESHOLD_DAYS = 3;

// ── Helper Functions ────────────────────────────────────────────────

export const getTodayStr = (): string => {
  const now = new Date();
  const offset = now.getTimezoneOffset() * 60000;
  const localNow = new Date(now.getTime() - offset);
  return localNow.toISOString().split('T')[0];
};

export const getPriorityClass = (priority: string): string => {
  switch (priority) {
    case 'P1': return 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400';
    case 'P2': return 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400';
    case 'P3': return 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400';
    case 'P4': return 'bg-slate-100 text-slate-700 dark:bg-slate-800/50 dark:text-slate-400';
    default: return '';
  }
};

export const isOverdue = (task: TaskMetadata): boolean => {
  if (task.status === 'done') return false;
  if (!task.due_date) return false;
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const dueDate = new Date(task.due_date);
  dueDate.setHours(0, 0, 0, 0);
  return dueDate < today;
};

export const formatNumber = (val: string | number | null | undefined): string | null => {
  if (!val) return null;
  const num = String(val).replace(/[^0-9.]/g, '');
  if (!num) return null;
  const parts = num.split('.');
  parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  return parts.join('.');
};

export const getTransferredName = (rawStr: string | null | undefined): string => {
  if (!rawStr) return 'Unknown';
  const match = rawStr.match(/^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/);
  return match ? `@${match[1]}` : rawStr;
};

export const isLinkedPerson = (rawStr: string | null | undefined): boolean => {
  return /^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/.test(rawStr || '');
};
