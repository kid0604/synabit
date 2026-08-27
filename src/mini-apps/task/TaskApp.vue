<script setup lang="ts">
import { computed, ref, onMounted, watch, toRef } from 'vue';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { useSettings } from '../../composables/useSettings';
import { CheckCircle2, Plus } from 'lucide-vue-next';
import { type TaskMetadata, BOARD_COLUMNS } from './types';
import { routeForNode } from '../../shared/nodeRoutes';

// ── Composables ────────────────────────────────────────────────────
import { useTaskCrud } from './composables/useTaskCrud';
import { useTaskSearch } from './composables/useTaskSearch';
import { useProjectManager } from './composables/useProjectManager';
import { useBoardLogic } from './composables/useBoardLogic';
import { useTaskSelection } from './composables/useTaskSelection';
import { useTaskKeyboard } from './composables/useTaskKeyboard';
import { useTaskBacklinks } from './composables/useTaskBacklinks';
import { useTaskFilters } from './composables/useTaskFilters';
import { sortTasks, groupTasks, type SortMode, type GroupMode } from './sorting';

// ── Components ─────────────────────────────────────────────────────
import TaskSidebar from './components/TaskSidebar.vue';
import TaskHeader from './components/TaskHeader.vue';
import TaskListView from './components/TaskListView.vue';
import TaskBoardView from './components/TaskBoardView.vue';
import TaskTableView from './components/TaskTableView.vue';
import TaskMatrixView from './components/TaskMatrixView.vue';
import ProjectDashboard from './components/ProjectDashboard.vue';

// ── Modals (existing) ──────────────────────────────────────────────
import TaskEditModal from './TaskEditModal.vue';
import ProjectEditModal from './ProjectEditModal.vue';
import ResourceLinkModal from './ResourceLinkModal.vue';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
import UndoToast from '../../shared/components/UndoToast.vue';
import TaskBulkBar from './components/TaskBulkBar.vue';
import TaskSortBar from './components/TaskSortBar.vue';
import SaveSearchButton from './components/SaveSearchButton.vue';
import TaskShortcutsHelp from './components/TaskShortcutsHelp.vue';
import TransactionModal from '../finance/TransactionModal.vue';

// ── Props & Emits ──────────────────────────────────────────────────
const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);
const { t: tt } = useI18n();

// ── Services ───────────────────────────────────────────────────────
const { taskArchiveDays, taskDeleteConfirm } = useSettings();
const showShortcuts = ref(false);
const bus = useEventBus();
const ns = useNodeService();
const vaultPathRef = toRef(props, 'vaultPath');

// ── Shared Refs (created at orchestrator level) ────────────────────
const tasks = ref<TaskMetadata[]>([]);
const projects = ref<any[]>([]);
const isMobileSidebarOpen = ref(false);

// ── 1. Search & Categories ─────────────────────────────────────────
const {
  searchQuery, activeCategory,
  categoryCounts, activeCategoryTasks,
} = useTaskSearch(tasks);

// ── 2. Project Manager ─────────────────────────────────────────────
const {
  activeProject, activeProjectTab,
  projectProgress, projectBudget, projectSpent, displayCustomFields,
  calculatedProjectSpent,
  linkedResources, loadProjectResources,
  showProjectEditModal, newProjectDraft,
  handleCreateProjectClick, handleProjectSave, deleteProject,
  showEmbedPicker, allNotesForPicker, isLinkingResource, showAddResourceMenu, showEmptyAddMenu,
  openLinkResourcePicker, createNewResourceNote, createNewResourceWhiteboard,
  unlinkResource, handleEmbedResource,
  showTxModal, incomeCategories, expenseCategories, accounts,
  loadFinanceConfig, saveFinanceTransaction,
} = useProjectManager(
  activeCategory, activeCategoryTasks, projects,
  ns, vaultPathRef, emit as any,
  async () => { /* loadTasks called from crud */ },
);

// ── 3. Board & Matrix Logic ────────────────────────────────────────
const {
  viewMode, quickAddColumn, quickAddTitle,
  WIP_LIMIT, tasksByStatus, tasksByQuadrant,
  showQuickAdd, handleQuickAdd,
  onDragStart, onDrop, onMatrixDrop,
} = useBoardLogic(
  tasks, activeCategoryTasks, activeCategory, activeProject,
  ns, (msg: string) => showToast(msg),
);

// ── 4. Task CRUD ───────────────────────────────────────────────────
const {
  editingTask, editingTaskParams,
  toastMessage, showToast,
  loadTasks, archiveDoneTasks,
  openEditModal, openCreateModal,
  handleModalSave, handleModalDelete,
  openEditById,
  toggleTaskStatus, deleteTask,
  pendingSubtreeDelete, answerSubtreeDelete,
  pendingDelete, undoDelete, deleteMany,
} = useTaskCrud(
  tasks, projects, vaultPathRef, ns, bus,
  activeCategory, activeProject, taskArchiveDays, taskDeleteConfirm,
  { tasksByStatus, WIP_LIMIT },
);

// ── 5. Sorting and grouping ────────────────────────────────────────
const { taskListSort, taskListGroup } = useSettings();

const projectTitle = (id: string) =>
  projects.value.find(p => p.id === id)?.title || id.split('/').pop() || id;

/** The list's tasks, arranged. Sorted before grouping, so sections agree. */
const sortedCategoryTasks = computed(() =>
  sortTasks(activeCategoryTasks.value, taskListSort.value as SortMode));

const listGroups = computed(() =>
  groupTasks(sortedCategoryTasks.value, taskListGroup.value as GroupMode, projectTitle));

// ── 6. Bulk selection ──────────────────────────────────────────────
/**
 * What the current view is actually showing.
 *
 * Not simply `activeCategoryTasks`: the matrix hides finished tasks and the
 * board groups by status. "Select all" has to mean the rows in front of the
 * user, and a task that is not on screen must not be able to sit in the
 * selection while a bulk action runs over it.
 */
const visibleInCurrentView = computed<TaskMetadata[]>(() => {
  if (viewMode.value === 'board') return Object.values(tasksByStatus.value).flat();
  if (viewMode.value === 'matrix') return Object.values(tasksByQuadrant.value).flat();
  return activeCategoryTasks.value;
});

const {
  selectedIds, selectedTasks, isSelected: _isSelected,
  selectOne, selectRange, clear: clearSelection,
  toggleAllVisible, allVisibleSelected,
  completeSelected, setPriorityOnSelection, setProjectOnSelection,
} = useTaskSelection(visibleInCurrentView, tasks, ns, showToast, tt);

const deleteSelected = async () => {
  const chosen = [...selectedTasks.value];
  if (!chosen.length) return;
  clearSelection();
  await deleteMany(chosen, tt('task.deleted_many_toast', { count: chosen.length }));
};

// ── 6b. Saved searches ─────────────────────────────────────────────
const {
  filters, load: loadFilters, save: saveFilter,
  rename: renameFilter, remove: removeFilter, byId: filterById,
} = useTaskFilters(ns, showToast, tt);

/**
 * Applying a saved search means the query *and* the arrangement.
 *
 * A filter restored without its view and grouping would need adjusting by hand
 * every time it was opened, which is most of what saving it was meant to avoid.
 */
watch(activeCategory, (category) => {
  if (!category.startsWith('filter:')) return;
  const filter = filterById(category.substring(7));
  if (!filter) return;
  searchQuery.value = filter.query;
  viewMode.value = filter.viewMode;
  taskListSort.value = filter.sort;
  taskListGroup.value = filter.group;
});

/**
 * Keep what is on screen, under a name.
 *
 * The name comes from the button itself; see `SaveSearchButton` for why it is
 * not a `window.prompt`.
 */
const saveCurrentSearch = async (name: string) => {
  const query = searchQuery.value.trim();
  if (!query || !name.trim()) return;
  const saved = await saveFilter({
    name: name.trim(),
    query,
    viewMode: viewMode.value,
    sort: taskListSort.value as SortMode,
    group: taskListGroup.value as GroupMode,
  });
  if (saved) activeCategory.value = 'filter:' + saved.id;
};

const renameFilterById = async (id: string, name: string) => {
  const filter = filterById(id);
  if (filter) await renameFilter(filter, name);
};

const deleteFilterById = async (id: string) => {
  const filter = filterById(id);
  if (!filter) return;
  await removeFilter(filter);
  if (activeCategory.value === 'filter:' + id) activeCategory.value = 'today';
};

/**
 * What to call whatever is on screen.
 *
 * One place, so a bucket added later cannot fall through to printing its own
 * id — which is what put `Filter:Filters/ac40aa52-…` across the top when saved
 * searches arrived.
 */
const BUCKET_TITLES: Record<string, string> = {
  all: 'task.all_tasks',
  today: 'task.today',
  upcoming: 'task.upcoming',
  someday: 'task.someday',
  transferred: 'task.transferred',
};

const headerTitle = computed(() => {
  const category = activeCategory.value;
  if (activeProject.value) return activeProject.value.title;
  if (category.startsWith('filter:')) {
    return filterById(category.substring(7))?.name ?? tt('task.filters');
  }
  const key = BUCKET_TITLES[category];
  // A project that has been deleted while selected leaves its category behind.
  return key ? tt(key) : tt('task.all_tasks');
});

// ── 7. Keyboard ────────────────────────────────────────────────────
/**
 * The rows the cursor walks, in the order they appear on screen.
 *
 * Only the linear views have one: a board has four columns side by side and a
 * matrix four quadrants, and "the next row" has no answer in either.
 */
const keyboardRows = computed<TaskMetadata[]>(() => {
  if (viewMode.value === 'list' || viewMode.value === 'table') {
    return listGroups.value.flatMap(g => g.tasks);
  }
  return [];
});


const { focusedId } = useTaskKeyboard(
  keyboardRows,
  computed(() => selectedTasks.value.length > 0),
  // A shortcut firing behind an open modal would act on a task the user cannot
  // see, so every one of them is suspended while something is up.
  computed(() => !!editingTask.value || !!pendingSubtreeDelete.value || showProjectEditModal.value || showShortcuts.value),
  {
    createTask: openCreateModal,
    openTask: openEditModal,
    toggleStatus: toggleTaskStatus,
    deleteTask,
    selectOne,
    selectRange,
    clearSelection,
    selectAllVisible: toggleAllVisible,
    focusSearch: () => document.getElementById('task-search-input')?.focus(),
    setViewMode: (mode) => { viewMode.value = mode; },
    showHelp: () => { showShortcuts.value = true; },
  },
);

// ── 8. Backlinks ───────────────────────────────────────────────────
const { backlinks, loading: backlinksLoading } = useTaskBacklinks(editingTask, ns);

// ── Navigation ─────────────────────────────────────────────────────
const openProjectById = (id: string) => {
  const normalizedId = id.replace(/\\/g, '/');
  const proj = projects.value.find(p => p.id.replace(/\\/g, '/') === normalizedId)
            || projects.value.find(p => p.id.replace(/\\/g, '/').endsWith(normalizedId));
  if (proj) {
    activeCategory.value = 'project:' + proj.id;
  } else {
    activeCategory.value = 'project:' + id;
  }
};

const refresh = async () => {
  await loadTasks(() => loadFinanceConfig());
  if (activeProject.value) {
    await loadProjectResources();
  }
};

/**
 * Follow a backlink to whatever owns it.
 *
 * Routed through the shared map rather than a local guess: the panel lists
 * notes, boards, people and events alongside tasks, and each goes to a
 * different mini-app. An unknown type opens nothing rather than opening the
 * note editor, which is how a task once ended up being edited as a note.
 */
const openBacklink = (id: string, nodeType: string) => {
  const route = routeForNode(nodeType, id);
  if (!route) return;
  if (route === 'task') {
    editingTask.value = null;
    void openEditById(id);
    return;
  }
  emit('open-node', id, route);
};

const openPerson = (transferredTo: string) => {
  if (!transferredTo) return;
  const match = transferredTo.match(/^\[(.*?)\]\(synabit:\/\/person\/(.*?)\)$/);
  if (match && match[2]) {
    emit('open-node', match[2], 'person');
  }
};

defineExpose({ openEditById, openProjectById, refresh });

// ── Lifecycle & Event Bus ──────────────────────────────────────────
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
const debouncedLoad = (fn: () => void, ms = 300) => {
  if (_debounceTimer) clearTimeout(_debounceTimer);
  _debounceTimer = setTimeout(fn, ms);
};

onMounted(() => {
  loadTasks(() => loadFinanceConfig());
  void loadFilters();
  // Once, on open — not on every watcher tick; see `archiveDoneTasks`. The
  // reload is worth a second round trip only when something actually moved.
  archiveDoneTasks().then((moved) => {
    if (moved > 0) loadTasks(() => loadFinanceConfig());
  });

  bus.on('vault:file-modified', () => {
    debouncedLoad(() => loadTasks(() => loadFinanceConfig()));
  });

  bus.on('vault:file-created-deleted', () => {
    debouncedLoad(() => loadTasks(() => loadFinanceConfig()));
  });

  bus.on('vault:sync-completed', () => {
    debouncedLoad(() => loadTasks(() => loadFinanceConfig()));
  });

  bus.on('node:created', ({ nodeType }: { nodeType: string }) => {
    if (nodeType === 'task' || nodeType === 'project') debouncedLoad(() => loadTasks(() => loadFinanceConfig()));
  });

  bus.on('node:deleted', ({ nodeType }: { nodeType: string }) => {
    if (nodeType === 'task' || nodeType === 'project') debouncedLoad(() => loadTasks(() => loadFinanceConfig()));
  });
});

watch(() => props.vaultPath, () => {
  loadTasks(() => loadFinanceConfig());
});
</script>

<template>
  <div class="h-full flex bg-[#fdfdfc] dark:bg-[#242424] w-full overflow-hidden">
    <!-- Desktop Sidebar -->
    <TaskSidebar
      variant="desktop"
      :activeCategory="activeCategory"
      :categoryCounts="categoryCounts"
      :projects="projects"
      @update:activeCategory="activeCategory = $event"
      :filters="filters"
      @create-project="handleCreateProjectClick"
      @delete-filter="deleteFilterById"
      @rename-filter="renameFilterById"
    />

    <!-- MAIN CONTENT -->
    <div class="flex-1 flex flex-col h-full overflow-hidden">
      <!-- Header -->
      <TaskHeader
        :title="headerTitle"
        :viewMode="viewMode"
        :searchQuery="searchQuery"
        @update:viewMode="viewMode = $event"
        @update:searchQuery="searchQuery = $event"
        @create-task="openCreateModal"
        @open-mobile-sidebar="isMobileSidebarOpen = true"
      >
        <template #save-search>
          <SaveSearchButton
            v-if="searchQuery.trim()"
            :suggestedName="searchQuery.trim()"
            @save="saveCurrentSearch"
          />
        </template>

        <template #sort>
          <TaskSortBar
            v-if="viewMode === 'list'"
            :sort="taskListSort as SortMode"
            :group="taskListGroup as GroupMode"
            @update:sort="taskListSort = $event"
            @update:group="taskListGroup = $event"
            @show-shortcuts="showShortcuts = true"
          />
        </template>
      </TaskHeader>

      <!-- Main Content -->
      <div class="flex-1 overflow-y-auto px-4 md:px-8 pb-16">

        <!-- Project Dashboard -->
        <ProjectDashboard
          v-if="activeProject"
          :activeProject="activeProject"
          :activeProjectTab="activeProjectTab"
          :activeCategoryTasks="activeCategoryTasks"
          :projectProgress="projectProgress"
          :projectBudget="projectBudget"
          :projectSpent="projectSpent"
          :displayCustomFields="displayCustomFields"
          :linkedResources="linkedResources"
          :isLinkingResource="isLinkingResource"
          :showAddResourceMenu="showAddResourceMenu"
          :showEmptyAddMenu="showEmptyAddMenu"
          @update:activeProjectTab="activeProjectTab = $event"
          @edit-project="showProjectEditModal = true"
          @show-tx-modal="showTxModal = true"
          @create-note="createNewResourceNote"
          @create-whiteboard="createNewResourceWhiteboard"
          @open-link-picker="openLinkResourcePicker"
          @unlink-resource="unlinkResource"
          @open-node="(id: string, type: string) => emit('open-node', id, type)"
          @update:showAddResourceMenu="showAddResourceMenu = $event"
          @update:showEmptyAddMenu="showEmptyAddMenu = $event"
        />

        <!-- Task Views -->
        <div v-show="!activeProject || activeProjectTab === 'tasks'" class="h-full flex-1 flex flex-col">
          <div v-if="activeCategoryTasks.length === 0" class="flex flex-col items-center justify-center h-48 opacity-40">
            <CheckCircle2 class="w-16 h-16 mb-4"/>
            <p>{{ $t('task.all_caught_up') }}</p>
          </div>

          <div v-else class="h-full flex flex-col min-h-0">
            <!-- LIST VIEW -->
            <TaskBulkBar
              v-if="selectedTasks.length"
              :selected="selectedTasks"
              :allVisibleSelected="allVisibleSelected"
              :projects="projects"
              @complete="completeSelected"
              @delete="deleteSelected"
              @set-priority="setPriorityOnSelection"
              @set-project="setProjectOnSelection"
              @toggle-all="toggleAllVisible"
              @clear="clearSelection"
            />

            <TaskListView
              v-if="viewMode === 'list'"
              :deleteConfirm="taskDeleteConfirm"
              :tasks="sortedCategoryTasks"
              :groups="listGroups"
              :allTasks="tasks"
              :selectedIds="selectedIds"
              :focusedId="focusedId"
              @edit-task="openEditModal"
              @toggle-status="toggleTaskStatus"
              @delete-task="deleteTask"
              @open-person="openPerson"
              @select-one="selectOne"
              @select-range="selectRange"
            />

            <!-- BOARD VIEW -->
            <TaskBoardView
              :deleteConfirm="taskDeleteConfirm"
              :allTasks="tasks"
              :selectedIds="selectedIds"
              @select-one="selectOne"
              v-else-if="viewMode === 'board'"
              :tasksByStatus="tasksByStatus"
              :columns="BOARD_COLUMNS"
              :wipLimit="WIP_LIMIT"
              :quickAddColumn="quickAddColumn"
              :quickAddTitle="quickAddTitle"
              @edit-task="openEditModal"
              @delete-task="deleteTask"
              @drag-start="(e: DragEvent, t: TaskMetadata) => onDragStart(e, t)"
              @drop="(e: DragEvent, status: string) => onDrop(e, status)"
              @show-quick-add="showQuickAdd"
              @quick-add="handleQuickAdd"
              @update:quickAddColumn="quickAddColumn = $event"
              @update:quickAddTitle="quickAddTitle = $event"
              @open-person="openPerson"
            />

            <!-- TABLE VIEW -->
            <TaskTableView
              :deleteConfirm="taskDeleteConfirm"
              :allTasks="tasks"
              :selectedIds="selectedIds"
              :focusedId="focusedId"
              @select-one="selectOne"
              @select-range="selectRange"
              v-else-if="viewMode === 'table'"
              :tasks="sortedCategoryTasks"
              @edit-task="openEditModal"
              @toggle-status="toggleTaskStatus"
              @delete-task="deleteTask"
              @open-person="openPerson"
            />

            <!-- MATRIX VIEW -->
            <TaskMatrixView
              :deleteConfirm="taskDeleteConfirm"
              :allTasks="tasks"
              :selectedIds="selectedIds"
              @select-one="selectOne"
              v-else-if="viewMode === 'matrix'"
              :tasksByQuadrant="tasksByQuadrant"
              @edit-task="openEditModal"
              @toggle-status="toggleTaskStatus"
              @delete-task="deleteTask"
              @drag-start="(e: DragEvent, t: TaskMetadata) => onDragStart(e, t)"
              @matrix-drop="(e: DragEvent, q: string) => onMatrixDrop(e, q)"
              @open-person="openPerson"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Edit Task Modal -->
    <TaskEditModal
      v-if="editingTask"
      :task="editingTaskParams"
      :vaultPath="vaultPath"
      :projects="projects"
      :allTasks="tasks"
      :taskId="editingTask?.id"
      :issues="editingTask?.issues"
      :backlinks="backlinks"
      :backlinksLoading="backlinksLoading"
      @open-node="openBacklink"
      @save="handleModalSave"
      @close="editingTask = null"
      @delete="handleModalDelete"
    />

    <!-- Edit Project Modal -->
    <ProjectEditModal
      v-if="showProjectEditModal && (activeProject || newProjectDraft)"
      :project="newProjectDraft || activeProject"
      :vaultPath="vaultPath"
      :dynamic-spent="calculatedProjectSpent"
      @save="handleProjectSave"
      @close="showProjectEditModal = false; newProjectDraft = null;"
      @delete="deleteProject"
    />

    <!-- Mobile Floating Action Button (FAB) -->
    <button
      @click="openCreateModal"
      class="md:hidden fixed bottom-20 right-6 z-[100] flex items-center justify-center w-14 h-14 bg-blue-500 text-white rounded-full shadow-[0_4px_20px_rgba(59,130,246,0.4)] hover:bg-blue-600 active:scale-95 transition-all"
     aria-label="Open Create Modal">
      <Plus class="w-6 h-6" />
    </button>

    <!-- Mobile Sidebar -->
    <TaskSidebar
      variant="mobile"
      :activeCategory="activeCategory"
      :categoryCounts="categoryCounts"
      :projects="projects"
      :isMobileOpen="isMobileSidebarOpen"
      @update:activeCategory="activeCategory = $event"
      :filters="filters"
      @create-project="handleCreateProjectClick"
      @delete-filter="deleteFilterById"
      @rename-filter="renameFilterById"
      @close-mobile="isMobileSidebarOpen = false"
    />

    <!-- WIP Notification Toast -->
    <transition
      enter-active-class="transition duration-300 ease-out"
      enter-from-class="transform translate-y-4 opacity-0"
      enter-to-class="transform translate-y-0 opacity-100"
      leave-active-class="transition duration-200 ease-in"
      leave-from-class="transform translate-y-0 opacity-100"
      leave-to-class="transform translate-y-4 opacity-0"
    >
      <div v-if="toastMessage" class="fixed bottom-8 left-1/2 -translate-x-1/2 bg-gray-900 dark:bg-white text-white dark:text-gray-900 px-5 py-3 rounded-xl shadow-xl z-[100] text-sm font-semibold flex items-center gap-2 max-w-md w-max pointer-events-none">
        {{ toastMessage }}
      </div>
    </transition>

    <!-- Transaction Modal (Finance Integration) -->
    <TransactionModal
      :show="showTxModal"
      :transaction="null"
      :income-categories="incomeCategories"
      :expense-categories="expenseCategories"
      :accounts="accounts"
      :projects="projects"
      :default-project-id="activeProject?.id"
      @close="showTxModal = false"
      @save="saveFinanceTransaction"
    />

    <!-- Deleting a task that has subtasks: cancel, keep them, or take them too -->
    <ConfirmModal
      :show="!!pendingSubtreeDelete"
      :title="$t('task.delete_parent_title', { title: pendingSubtreeDelete?.task.title || '' })"
      :message="$t('task.delete_parent_body', { count: pendingSubtreeDelete?.count || 0 })"
      :confirmText="$t('task.delete_all_subtasks')"
      :secondaryText="$t('task.delete_keep_subtasks')"
      :cancelText="$t('task.delete_cancel')"
      isDestructive
      @confirm="answerSubtreeDelete('all')"
      @secondary="answerSubtreeDelete('keep')"
      @cancel="answerSubtreeDelete(null)"
    />

    <!-- The few seconds in which a delete can still be taken back -->
    <UndoToast
      :show="!!pendingDelete"
      :restartKey="pendingDelete?.removed[0]?.task.id"
      :message="pendingDelete?.removed.length === 1
        ? $t('task.deleted_toast', { title: pendingDelete.label })
        : $t('task.deleted_many_toast', { count: pendingDelete?.removed.length || 0 })"
      :undoLabel="$t('task.undo')"
      :seconds="7"
      @undo="undoDelete"
    />

    <TaskShortcutsHelp :show="showShortcuts" @close="showShortcuts = false" />

    <ResourceLinkModal
      :show="showEmbedPicker"
      :available-nodes="allNotesForPicker"
      @close="showEmbedPicker = false"
      @select="handleEmbedResource"
    />
  </div>
</template>
