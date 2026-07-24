<script setup lang="ts">
import { ref, onMounted, watch, toRef } from 'vue';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { useSettings } from '../../composables/useSettings';
import { CheckCircle2, Plus } from 'lucide-vue-next';
import { type TaskMetadata, BOARD_COLUMNS } from './types';

// ── Composables ────────────────────────────────────────────────────
import { useTaskCrud } from './composables/useTaskCrud';
import { useTaskSearch } from './composables/useTaskSearch';
import { useProjectManager } from './composables/useProjectManager';
import { useBoardLogic } from './composables/useBoardLogic';

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
import TransactionModal from '../finance/TransactionModal.vue';

// ── Props & Emits ──────────────────────────────────────────────────
const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);

// ── Services ───────────────────────────────────────────────────────
const { taskArchiveDays } = useSettings();
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
} = useTaskSearch(tasks, vaultPathRef);

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
  loadTasks,
  openEditModal, openCreateModal,
  handleModalSave, handleModalDelete,
  openEditById,
  toggleTaskStatus, deleteTask,
} = useTaskCrud(
  tasks, projects, vaultPathRef, ns, bus,
  activeCategory, activeProject, taskArchiveDays,
  { tasksByStatus, WIP_LIMIT },
);

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
      @create-project="handleCreateProjectClick"
    />

    <!-- MAIN CONTENT -->
    <div class="flex-1 flex flex-col h-full overflow-hidden">
      <!-- Header -->
      <TaskHeader
        :activeProject="activeProject"
        :activeCategory="activeCategory"
        :viewMode="viewMode"
        :searchQuery="searchQuery"
        @update:viewMode="viewMode = $event"
        @update:searchQuery="searchQuery = $event"
        @create-task="openCreateModal"
        @open-mobile-sidebar="isMobileSidebarOpen = true"
      />

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
            <TaskListView
              v-if="viewMode === 'list'"
              :tasks="activeCategoryTasks"
              @edit-task="openEditModal"
              @toggle-status="toggleTaskStatus"
              @delete-task="deleteTask"
              @open-person="openPerson"
            />

            <!-- BOARD VIEW -->
            <TaskBoardView
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
              v-else-if="viewMode === 'table'"
              :tasks="activeCategoryTasks"
              @edit-task="openEditModal"
              @toggle-status="toggleTaskStatus"
              @delete-task="deleteTask"
              @open-person="openPerson"
            />

            <!-- MATRIX VIEW -->
            <TaskMatrixView
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
      @create-project="handleCreateProjectClick"
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

    <ResourceLinkModal
      :show="showEmbedPicker"
      :available-nodes="allNotesForPicker"
      @close="showEmbedPicker = false"
      @select="handleEmbedResource"
    />
  </div>
</template>
