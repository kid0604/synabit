<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted, watch } from 'vue';
import { useNodeService } from '../../composables/useNodeService';
import { CheckCircle2, Calendar, Tag, Flag, X, Send, Eye, EyeOff, Trash2, Plus, Bell, Repeat, CornerDownRight, TriangleAlert } from 'lucide-vue-next';
import TiptapEditor from '../note/TiptapEditor.vue';
import { getTodayStr, REMINDER_PRESETS, isValidReminder } from './types';
import { RECURRENCE_OPTIONS } from './recurrence';
import type { FieldIssue } from './validation';
import type { Backlink } from './composables/useTaskBacklinks';
import TaskBacklinks from './components/TaskBacklinks.vue';
import { eligibleParents } from './subtasks';
import { useI18n } from 'vue-i18n';

const props = defineProps<{
    task: any;
    vaultPath?: string;
    showActions?: boolean;
    projects?: any[];
    /** Every task, so the parent picker can offer them and exclude descendants. */
    allTasks?: any[];
    /**
     * Fields the file holds a nonsense value for; see `validation.ts`.
     *
     * Named here rather than read off `task`, because `task` is the flattened
     * form state and the guards have already replaced the values it carries.
     */
    issues?: FieldIssue[];
    /**
     * The edited task's own id.
     *
     * Separate from `task`, which is the flattened form state and carries no
     * id — without this the parent picker cannot tell which task is being
     * edited, and would happily offer the task itself and its own children,
     * which is exactly the cycle it exists to prevent.
     */
    taskId?: string;
    /** What else in the vault points at this task. */
    backlinks?: Backlink[];
    backlinksLoading?: boolean;
}>();

const emit = defineEmits(['save', 'close', 'delete', 'open-node']);
const { t } = useI18n();
const ns = useNodeService();

// Create a reactive clone of the passed task params
const editingTaskParams = ref({
    title: props.task?.title || '',
    content: props.task?.content || '',
    is_transferred: props.task?.is_transferred || false,
    transferred_to: props.task?.transferred_to || '',
    track_progress: props.task?.track_progress || false,
    priority: props.task?.priority || '',
    start_date: props.task?.start_date || '',
    due_date: props.task?.due_date || '',
    due_time: props.task?.due_time || '',
    reminders: [...(props.task?.reminders || [])] as string[],
    recurrence: props.task?.recurrence || 'none',
    recurrence_end_at: props.task?.recurrence_end_at || '',
    parent_id: props.task?.parent_id || '',
    comment: props.task?.comment || '',
    tags: props.task?.tags || '',
    status: props.task?.status || 'todo',
    project_id: props.task?.project_id || ''
});

// Imported rather than redefined: the local copy used `toISOString`, so in
// UTC+7 this form labelled tomorrow's date "Today" for the whole evening.

const activeDropdown = ref<string | null>(null);

// ── Reminders ──────────────────────────────────────────────────────
const reminderPreset = ref('');
const customReminder = ref('');
const reminderError = ref('');

const addReminder = () => {
    const raw = reminderPreset.value === 'custom' ? customReminder.value : reminderPreset.value;
    const value = raw.trim().toLowerCase();
    if (!value) return;
    if (!isValidReminder(value)) {
        // Shown in the panel rather than through `alert`, which is what the
        // Calendar's copy of this does — a modal dialog on top of a modal
        // dismisses the dropdown behind it and loses what was typed.
        reminderError.value = t('task.reminder_invalid');
        return;
    }
    if (!editingTaskParams.value.reminders.includes(value)) {
        editingTaskParams.value.reminders.push(value);
    }
    reminderPreset.value = '';
    customReminder.value = '';
    reminderError.value = '';
};

const removeReminder = (idx: number) => {
    editingTaskParams.value.reminders.splice(idx, 1);
};

/**
 * A reminder with no deadline to count back from never fires, and the loop in
 * `chat_engine.rs` only ever looks at tasks that have a `due_date`. Saying so
 * beats letting the user set one that silently does nothing.
 */
const remindersNeedDueDate = computed(
    () => editingTaskParams.value.reminders.length > 0 && !editingTaskParams.value.due_date,
);

// ── Repeat ─────────────────────────────────────────────────────────
const isRepeating = computed(() => editingTaskParams.value.recurrence !== 'none');

/**
 * A repeat counts forward from a date. With neither a start nor a due date
 * there is nothing to count from, and `advanceRecurrence` finishes the task
 * instead of moving it on — so the repeat would silently do nothing.
 */
const repeatNeedsDate = computed(
    () => isRepeating.value && !editingTaskParams.value.due_date && !editingTaskParams.value.start_date,
);

// ── Parent ─────────────────────────────────────────────────────────
/**
 * The task itself and everything under it are excluded, because either would
 * make a cycle. This picker is the only place a cycle could be created on
 * purpose, so it is the place to prevent one.
 */
const parentOptions = computed(() => {
    const all = (props.allTasks || []) as any[];
    if (!all.length) return [];
    // A task being created has no id yet and no children, so nothing to exclude.
    return eligibleParents({ id: props.taskId || '' } as any, all as any);
});

const parentTitle = computed(() => {
    const id = editingTaskParams.value.parent_id;
    if (!id) return '';
    const found = (props.allTasks || []).find((t: any) => t.id === id);
    return found?.title || id.split('/').pop() || id;
});

const handleGlobalClick = () => {
    activeDropdown.value = null;
};

const titleInputRef = ref<HTMLTextAreaElement | null>(null);
const tiptapRef = ref<any>(null);

const handleTitleEnter = () => {
    if (tiptapRef.value) {
        tiptapRef.value.focus();
    }
};

const adjustTitleHeight = () => {
    nextTick(() => {
        if (titleInputRef.value) {
            titleInputRef.value.style.height = 'auto';
            titleInputRef.value.style.height = titleInputRef.value.scrollHeight + 'px';
        }
    });
};

const people = ref<any[]>([]);
const transferInput = ref('');
const isEditingTransfer = ref(false);

watch(() => editingTaskParams.value.transferred_to, (newVal) => {
    if (isEditingTransfer.value) return;
    const match = newVal.match(/^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/);
    transferInput.value = match ? match[1] : newVal;
}, { immediate: true });

const onTransferInput = (e: Event) => {
    isEditingTransfer.value = true;
    const val = (e.target as HTMLInputElement).value;
    transferInput.value = val;
    editingTaskParams.value.transferred_to = val;
};

const displayTransferredName = computed(() => {
    const rawStr = editingTaskParams.value.transferred_to;
    if (!rawStr) return '';
    const match = rawStr.match(/^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/);
    return match ? `@${match[1]}` : rawStr;
});

const filteredPeople = computed(() => {
    const query = transferInput.value.toLowerCase().trim();
    if (!query) return people.value;
    return people.value.filter(p => {
        if (p.title.toLowerCase().includes(query)) return true;
        if (p.properties?.nickname && p.properties.nickname.toLowerCase().includes(query)) return true;
        return false;
    });
});

const selectPerson = (person: any) => {
    isEditingTransfer.value = false;
    editingTaskParams.value.transferred_to = `[${person.title}](synabit://person/${person.id})`;
    transferInput.value = person.title;
    activeDropdown.value = null;
};

const createNewPerson = async () => {
    isEditingTransfer.value = false;
    const name = transferInput.value.trim();
    if (!name) return;
    try {
        const id = crypto.randomUUID();
        await ns.writeNode({
            relPath: `People/${id}.md`,
            title: name,
            nodeType: 'person',
            properties: { tags: [], is_owner: false, details: [] },
            content: '',
            eventType: 'created',
        });
        editingTaskParams.value.transferred_to = `[${name}](synabit://person/${id})`;
        transferInput.value = name;
        activeDropdown.value = null;
        people.value = await ns.getNodes('person');
    } catch (e) {
        console.error("Failed to create person", e);
    }
};

const usePlainText = () => {
    isEditingTransfer.value = false;
    editingTaskParams.value.transferred_to = transferInput.value.trim();
    activeDropdown.value = null;
};

const toggleTransferDropdown = () => {
    if (!editingTaskParams.value.is_transferred) {
        editingTaskParams.value.is_transferred = true;
        activeDropdown.value = 'transfer';
    } else {
        activeDropdown.value = activeDropdown.value === 'transfer' ? null : 'transfer';
    }
};

const clearTransfer = () => {
    editingTaskParams.value.is_transferred = false;
    editingTaskParams.value.transferred_to = '';
    editingTaskParams.value.track_progress = false;
    transferInput.value = '';
    activeDropdown.value = null;
};

onMounted(async () => {
    document.addEventListener('click', handleGlobalClick);
    adjustTitleHeight();
    try {
        people.value = await ns.getNodes('person');
    } catch (e) {
        console.error("Failed to load people", e);
    }
});

onUnmounted(() => {
    document.removeEventListener('click', handleGlobalClick);
});

const save = () => {
    // A time with no date is not a deadline, and the reminder loop would never
    // look at it. Clearing the date clears the time with it rather than leaving
    // an orphan in the frontmatter that reappears the next time a date is set.
    if (!editingTaskParams.value.due_date) {
        editingTaskParams.value.due_time = '';
    }
    if (editingTaskParams.value.is_transferred && !editingTaskParams.value.transferred_to.trim()) {
        editingTaskParams.value.is_transferred = false;
        editingTaskParams.value.track_progress = false;
        editingTaskParams.value.transferred_to = '';
    }
    emit('save', editingTaskParams.value);
};

const close = () => {
    emit('close');
};

const handleBackgroundClick = () => {
    if (props.showActions) {
        close();
    } else {
        save();
    }
};
</script>

<template>
  <div class="fixed inset-0 z-[110] flex items-center justify-center md:p-4 bg-black/10 dark:bg-black/40 backdrop-blur-[2px]" @mousedown.self="handleBackgroundClick">
      <div class="w-full h-full md:h-auto md:max-w-lg bg-white dark:bg-[#1e1e1e] md:rounded-2xl shadow-none md:shadow-[0_20px_40px_rgba(0,0,0,0.1)] md:dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border-none md:border md:border-gray-100 md:dark:border-[#2c2c2c] overflow-hidden flex flex-col" @mousedown.stop>
          
          <!-- Mobile Header -->
          <div class="flex justify-between items-center px-5 pb-4 md:hidden shrink-0 border-b border-gray-100 dark:border-[#2c2c2c]" style="padding-top: max(env(safe-area-inset-top), 36px);">
              <h3 class="font-semibold text-lg text-[#1c1c1e] dark:text-[#f4f4f5]">{{ props.showActions ? 'New Task' : 'Edit Task' }}</h3>
              <button @click="handleBackgroundClick" class="p-2 -mr-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]" aria-label="Handle Background Click">
                  <X class="w-4 h-4" />
              </button>
          </div>

          <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
              
              <!-- Title & Checkbox -->
              <div class="flex items-start gap-4 mb-3">
                   <button @click="editingTaskParams.status = (editingTaskParams.status === 'done' ? 'todo' : 'done')" class="shrink-0 mt-0.5 cursor-pointer" :aria-label="$t('task.a11y_toggle_status')">
                       <div v-if="editingTaskParams.status === 'done'" class="w-5 h-5 rounded border border-gray-300 dark:border-gray-600 bg-gray-100 dark:bg-[#2c2c2c] flex items-center justify-center">
                           <div class="w-2.5 h-2.5 bg-gray-400 dark:bg-gray-500 rounded-sm"></div>
                       </div>
                       <div v-else class="w-5 h-5 rounded border-[1.5px] border-gray-300 dark:border-gray-600 hover:border-gray-400 dark:hover:border-gray-500 transition-colors"></div>
                   </button>
                   <textarea 
                       ref="titleInputRef"
                       v-model="editingTaskParams.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 focus:ring-0 p-0 resize-none overflow-hidden leading-snug"
                       placeholder="New Task"
                       rows="1"
                       @input="adjustTitleHeight"
                       @keydown.enter.prevent="handleTitleEnter"
                   ></textarea>
              </div>
              
              <div class="pl-9 mb-4 flex-1 flex flex-col min-h-[40px] max-h-[300px] overflow-y-auto overflow-x-hidden custom-scrollbar">
                  <TiptapEditor 
                       ref="tiptapRef"
                       v-model="editingTaskParams.content" 
                       :vaultPath="props.vaultPath || ''"
                       :minHeightClass="'min-h-[40px]'"
                       class="w-full flex-1"
                  />
              </div>
              
          </div>
          
          <!--
            What the file says where the form cannot show it. The value is
            printed verbatim so the user can recognise their own two edits
            inside it, and saving the form writes a clean value over the top.
          -->
          <div v-if="issues?.length" class="mx-5 mb-3 px-3 py-2.5 rounded-lg bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900/50">
            <p class="flex items-center gap-1.5 text-[12px] font-semibold text-amber-800 dark:text-amber-300 mb-1">
              <TriangleAlert class="w-3.5 h-3.5 shrink-0" /> {{ t('task.field_issues_title') }}
            </p>
            <p v-for="issue in issues" :key="issue.field" class="text-[11px] text-amber-700 dark:text-amber-400/90 leading-relaxed">
              {{ t('task.field_issue_detail', { field: issue.field, value: issue.value }) }}
            </p>
          </div>

          <!--
            Only for a task that exists. A task being created has no identity
            for anything to point at yet, and an empty panel on the create
            screen is a promise the form cannot keep.
          -->
          <TaskBacklinks
            v-if="taskId"
            :backlinks="backlinks || []"
            :loading="!!backlinksLoading"
            @open="(id, nodeType) => emit('open-node', id, nodeType)"
          />

          <!-- Footer Meta Bar -->
          <div class="px-5 pt-3 border-t border-gray-50 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e] flex items-center justify-start gap-2 flex-wrap relative" :style="!props.showActions ? 'padding-bottom: max(env(safe-area-inset-bottom), 12px);' : 'padding-bottom: 12px;'">
              <!-- Dates -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="(editingTaskParams.start_date || editingTaskParams.due_date) ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Dates" @click.stop="activeDropdown = activeDropdown === 'dates' ? null : 'dates'">
                  <Calendar class="w-[18px] h-[18px]" :class="(editingTaskParams.start_date || editingTaskParams.due_date) ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingTaskParams.start_date || editingTaskParams.due_date" class="text-xs font-semibold">
                      <template v-if="editingTaskParams.start_date && editingTaskParams.due_date">
                          {{ editingTaskParams.start_date === getTodayStr() ? 'Today' : editingTaskParams.start_date }} &rarr; {{ editingTaskParams.due_date === getTodayStr() ? 'Today' : editingTaskParams.due_date }}
                      </template>
                      <template v-else-if="editingTaskParams.start_date">
                          {{ editingTaskParams.start_date === getTodayStr() ? 'Today' : editingTaskParams.start_date }}
                      </template>
                      <template v-else-if="editingTaskParams.due_date">
                          Due: {{ editingTaskParams.due_date === getTodayStr() ? 'Today' : editingTaskParams.due_date }}<template v-if="editingTaskParams.due_time"> {{ editingTaskParams.due_time }}</template>
                      </template>
                  </span>
                  
                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'dates' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-48 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Start Date</label>
                          <input type="date" v-model="editingTaskParams.start_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 mb-3 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" aria-label="Start date" />
                          
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Due Date</label>
                          <input type="date" v-model="editingTaskParams.due_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 mb-3 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" aria-label="Due date" />

                          <label class="block text-xs font-semibold text-gray-500 mb-1">{{ t('task.due_time_label') }}</label>
                          <input type="time" v-model="editingTaskParams.due_time" :disabled="!editingTaskParams.due_date" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-blue-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed" :aria-label="t('task.due_time_label')" />
                          <p class="text-[10px] text-gray-400 mt-1">{{ t('task.due_time_hint') }}</p>
                      </div>
                  </div>
              </div>

              <!-- Repeat -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="isRepeating ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" :title="t('task.repeat')" @click.stop="activeDropdown = activeDropdown === 'repeat' ? null : 'repeat'">
                  <Repeat class="w-[18px] h-[18px]" :class="isRepeating ? (repeatNeedsDate ? 'text-amber-500 mr-2' : 'text-teal-500 mr-2') : ''" />

                  <span v-if="isRepeating" class="text-xs font-semibold">{{ t('task.' + editingTaskParams.recurrence) }}</span>

                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'repeat' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-56 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">{{ t('task.repeat') }}</label>
                          <select v-model="editingTaskParams.recurrence" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-teal-500 cursor-pointer text-[#1c1c1e] dark:text-[#f4f4f5]" :aria-label="t('task.repeat')">
                              <option v-for="option in RECURRENCE_OPTIONS" :key="option" :value="option">
                                  {{ option === 'none' ? t('task.does_not_repeat') : t('task.' + option) }}
                              </option>
                          </select>

                          <template v-if="isRepeating">
                              <label class="block text-xs font-semibold text-gray-500 mb-1 mt-3">{{ t('task.repeat_until') }}</label>
                              <input type="date" v-model="editingTaskParams.recurrence_end_at" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-teal-500 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" :aria-label="t('task.repeat_until')" />
                              <p class="text-[10px] text-gray-400 mt-1">{{ t('task.repeat_until_hint') }}</p>
                              <p v-if="repeatNeedsDate" class="text-[10px] text-amber-600 dark:text-amber-500 mt-1.5">{{ t('task.repeat_needs_date') }}</p>
                          </template>
                      </div>
                  </div>
              </div>

              <!-- Parent task -->
              <div v-if="parentOptions.length" class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.parent_id ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" :title="t('task.parent_task')">
                  <CornerDownRight class="w-[18px] h-[18px]" :class="editingTaskParams.parent_id ? 'text-sky-500 mr-2' : ''" />

                  <span v-if="editingTaskParams.parent_id" class="text-xs font-semibold max-w-[120px] truncate text-sky-600 dark:text-sky-400">{{ parentTitle }}</span>

                  <select v-model="editingTaskParams.parent_id" class="absolute inset-0 opacity-0 cursor-pointer z-10" :aria-label="t('task.parent_task')">
                      <option value="">{{ t('task.no_parent') }}</option>
                      <option v-for="candidate in parentOptions" :key="candidate.id" :value="candidate.id">{{ candidate.title }}</option>
                  </select>
              </div>

              <!-- Reminders -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.reminders.length > 0 ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" :title="t('task.reminders')" @click.stop="activeDropdown = activeDropdown === 'reminders' ? null : 'reminders'">
                  <Bell class="w-[18px] h-[18px]" :class="editingTaskParams.reminders.length > 0 ? (remindersNeedDueDate ? 'text-amber-500 mr-2' : 'text-purple-500 mr-2') : ''" />

                  <span v-if="editingTaskParams.reminders.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingTaskParams.reminders.join(', ') }}</span>

                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'reminders' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-64 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-2">{{ t('task.reminders') }}</label>

                          <div v-if="editingTaskParams.reminders.length" class="flex items-center gap-1.5 flex-wrap mb-2">
                              <span v-for="(rem, idx) in editingTaskParams.reminders" :key="rem" class="flex items-center gap-1 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 px-2 py-1 rounded-md text-xs font-medium">
                                  <Bell class="w-3 h-3" />
                                  {{ rem }}
                                  <button @click="removeReminder(idx)" class="hover:text-purple-900 dark:hover:text-purple-100 ml-0.5 cursor-pointer" :aria-label="t('task.a11y_remove_reminder')">
                                      <X class="w-3 h-3" />
                                  </button>
                              </span>
                          </div>

                          <select v-model="reminderPreset" @change="reminderPreset !== 'custom' && addReminder()" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-purple-500 cursor-pointer text-[#1c1c1e] dark:text-[#f4f4f5]" :aria-label="t('task.add_reminder')">
                              <option value="">{{ t('task.add_reminder') }}</option>
                              <option value="0m">{{ t('task.reminder_at_due') }}</option>
                              <option v-for="preset in REMINDER_PRESETS" :key="preset" :value="preset">{{ t('task.reminder_before', { value: preset }) }}</option>
                              <option value="custom">{{ t('task.reminder_custom') }}</option>
                          </select>

                          <div v-if="reminderPreset === 'custom'" class="flex items-center gap-1.5 mt-2">
                              <input v-model="customReminder" @keyup.enter="addReminder" type="text" :placeholder="t('task.reminder_custom_placeholder')" class="flex-1 min-w-0 text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-purple-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                              <button @click="addReminder" class="bg-purple-600 hover:bg-purple-700 text-white p-1.5 rounded-md transition-colors cursor-pointer shrink-0" :aria-label="t('task.a11y_add_reminder')">
                                  <Plus class="w-4 h-4" />
                              </button>
                          </div>

                          <p v-if="reminderError" class="text-[10px] text-red-500 mt-1.5">{{ reminderError }}</p>
                          <p v-else-if="remindersNeedDueDate" class="text-[10px] text-amber-600 dark:text-amber-500 mt-1.5">{{ t('task.reminders_need_due_date') }}</p>
                      </div>
                  </div>
              </div>

              <!-- Tags -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.tags.length > 0 ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Manage Tags" @click.stop="activeDropdown = activeDropdown === 'tags' ? null : 'tags'">
                  <Tag class="w-[18px] h-[18px]" :class="editingTaskParams.tags.length > 0 ? 'text-blue-500 mr-2' : ''"/>
                  
                  <span v-if="editingTaskParams.tags.length > 0" class="text-xs font-semibold max-w-[150px] truncate">{{ editingTaskParams.tags }}</span>
                  
                  <div class="absolute bottom-full left-0 pb-2 transition-all z-50" :class="activeDropdown === 'tags' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-56 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-3 pointer-events-auto cursor-default">
                          <label class="block text-xs font-semibold text-gray-500 mb-1">Tags (comma separated)</label>
                          <input v-model="editingTaskParams.tags" placeholder="e.g. work, urgent" class="w-full text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-2 outline-none focus:ring-1 focus:ring-blue-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                      </div>
                  </div>
              </div>

              <!-- Priority -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.priority ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Priority">
                  <Flag class="w-[18px] h-[18px]" :class="editingTaskParams.priority ? 'text-orange-500 mr-2' : ''" />
                  
                  <span v-if="editingTaskParams.priority" class="text-xs font-semibold uppercase text-orange-600 dark:text-orange-400">{{ editingTaskParams.priority }}</span>
                  
                  <select v-model="editingTaskParams.priority" class="absolute inset-0 opacity-0 cursor-pointer z-10">
                      <option value="">None</option>
                      <option value="P1">P1</option>
                      <option value="P2">P2</option>
                      <option value="P3">P3</option>
                      <option value="P4">P4</option>
                  </select>
              </div>

              <!-- Project -->
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer group" :class="editingTaskParams.project_id ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" title="Set Project">
                  <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" :class="editingTaskParams.project_id ? 'text-indigo-500 mr-2' : ''"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                  
                  <span v-if="editingTaskParams.project_id" class="text-xs font-semibold max-w-[100px] truncate text-indigo-600 dark:text-indigo-400">
                      {{ props.projects?.find(p => p.id === editingTaskParams.project_id)?.title || 'Project' }}
                  </span>
                  
                  <select v-model="editingTaskParams.project_id" class="absolute inset-0 opacity-0 cursor-pointer z-10">
                      <option value="">No Project</option>
                      <option v-for="proj in props.projects" :key="proj.id" :value="proj.id">{{ proj.title }}</option>
                  </select>
              </div>

              <!-- Transfer -->
              <div class="relative flex items-center group">
                  <button 
                      @click.stop="toggleTransferDropdown"
                      class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] cursor-pointer flex items-center transition-colors" 
                      :class="editingTaskParams.is_transferred ? 'bg-gray-50 dark:bg-[#2a2a2a] px-2 text-[#1c1c1e] dark:text-[#f4f4f5]' : 'justify-center text-gray-400'" 
                      title="Transfer Task"
                  >
                      <Send class="w-[18px] h-[18px]" :class="editingTaskParams.is_transferred ? 'text-purple-500 mr-2' : ''" />
                      <span v-if="editingTaskParams.is_transferred && editingTaskParams.transferred_to" class="text-xs font-semibold max-w-[120px] truncate text-purple-600 dark:text-purple-400">
                          {{ displayTransferredName }}
                      </span>
                  </button>
                  
                  <div v-if="editingTaskParams.is_transferred" class="absolute bottom-full left-1/2 -translate-x-1/2 pb-2 transition-all z-50" :class="activeDropdown === 'transfer' ? 'opacity-100 visible' : 'opacity-0 invisible md:group-hover:opacity-100 md:group-hover:visible'" @click.stop>
                      <div class="w-64 bg-white dark:bg-[#1e1e1e] border border-gray-200 dark:border-[#2c2c2c] rounded-xl shadow-[0_4px_20px_rgb(0,0,0,0.15)] flex flex-col p-2 pointer-events-auto cursor-default items-center">
                          <label class="block text-[10px] font-semibold text-gray-400 mb-1 w-full text-left ml-1">Transfer to:</label>
                          <div class="flex items-center gap-1.5 w-full">
                              <input :value="transferInput" @input="onTransferInput" @click="activeDropdown = 'transfer'" placeholder="Name..." class="flex-1 min-w-0 text-sm bg-gray-50 dark:bg-[#2c2c2c] border border-gray-100 dark:border-gray-700 rounded-md p-1.5 outline-none focus:ring-1 focus:ring-purple-500 text-[#1c1c1e] dark:text-[#f4f4f5]" />
                              
                              <button 
                                  @click.stop="editingTaskParams.track_progress = !editingTaskParams.track_progress"
                                  class="p-1.5 rounded-md hover:opacity-80 transition-opacity shrink-0 flex items-center justify-center border"
                                  :title="editingTaskParams.track_progress ? 'Tracking Progress' : 'Not Tracking'"
                                  :class="editingTaskParams.track_progress ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-500 border-blue-200 dark:border-blue-800' : 'text-gray-400 dark:text-gray-500 bg-gray-50 dark:bg-[#2a2a2a] border-gray-200 dark:border-[#2c2c2c]'"
                              >
                                  <Eye v-if="editingTaskParams.track_progress" class="w-4 h-4" />
                                  <EyeOff v-else class="w-4 h-4" />
                              </button>
                              
                              <button 
                                  @click.stop="clearTransfer"
                                  class="p-1.5 rounded-md hover:bg-red-50 hover:text-red-500 dark:hover:bg-red-900/20 dark:hover:text-red-400 transition-colors shrink-0 flex items-center justify-center text-gray-400 dark:text-gray-500 border border-transparent"
                                  title="Remove Transfer"
                              >
                                  <X class="w-4 h-4" />
                              </button>
                          </div>

                          <div v-if="activeDropdown === 'transfer'" class="w-full mt-2 max-h-48 overflow-y-auto custom-scrollbar flex flex-col gap-1 border-t border-gray-100 dark:border-[#2c2c2c] pt-2">
                              <button v-for="p in filteredPeople" :key="p.id" @click.stop="selectPerson(p)" class="text-left px-2 py-1.5 text-xs text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] rounded-md truncate transition-colors shrink-0">
                                  {{ p.title }}
                              </button>
                              <div v-if="filteredPeople.length > 0" class="h-px bg-gray-100 dark:bg-[#2c2c2c] my-1 shrink-0"></div>
                              <button v-if="transferInput.trim() && !filteredPeople.find(p => p.title.toLowerCase() === transferInput.trim().toLowerCase())" @click.stop="createNewPerson" class="text-left px-2 py-1.5 text-xs text-purple-600 dark:text-purple-400 hover:bg-purple-50 dark:hover:bg-purple-900/20 rounded-md truncate transition-colors flex items-center gap-1 shrink-0">
                                  <Plus class="w-3 h-3" /> Create: "{{ transferInput }}"
                              </button>
                              <button v-if="transferInput.trim()" @click.stop="usePlainText" class="text-left px-2 py-1.5 text-xs text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#2c2c2c] rounded-md truncate transition-colors shrink-0">
                                  Use text: "{{ transferInput }}"
                              </button>
                          </div>
                      </div>
                  </div>
              </div>

              <!-- Delete Button (Only when editing existing task, i.e., !props.showActions) -->
              <div v-if="!props.showActions" class="ml-auto relative flex items-center p-1.5 rounded-md hover:bg-red-50 dark:hover:bg-red-900/20 text-red-400 hover:text-red-500 cursor-pointer transition-colors" title="Delete Task" @click.stop="emit('delete')">
                  <Trash2 class="w-[18px] h-[18px]" />
              </div>
          </div>

          <!-- Bottom Actions (Only for Convert mode) -->
          <div v-if="props.showActions" class="pt-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-end gap-3 shrink-0" style="padding-bottom: max(env(safe-area-inset-bottom), 16px);">
              <button @click="close" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer border border-transparent">
                  Cancel
              </button>
              <button @click="save" class="px-5 py-2 bg-blue-600 hover:bg-blue-700 dark:bg-blue-500 dark:hover:bg-blue-600 text-white rounded-lg text-sm font-medium transition-all shadow-sm cursor-pointer flex items-center gap-1.5 border border-transparent active:scale-95">
                  <CheckCircle2 class="w-4 h-4" /> Create Task
              </button>
          </div>
      </div>
  </div>
</template>
