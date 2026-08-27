<script setup lang="ts">
import { ref, nextTick } from 'vue';
import { Inbox, Sun, Calendar, Coffee, Send, Plus, X, Filter, Pencil} from 'lucide-vue-next';

const props = defineProps<{
  /** Saved searches, shown below the projects. */
  filters?: { id: string; name: string }[];
  activeCategory: string;
  categoryCounts: { all: number; today: number; upcoming: number; someday: number; transferred: number };
  projects: any[];
  isMobileOpen?: boolean;
  variant: 'desktop' | 'mobile';
}>();

/**
 * The saved search being renamed, if any.
 *
 * Typed in place rather than through a dialog, the same way the search was
 * named when it was saved — and for the same reason: this app's WebView has no
 * text prompt to fall back on.
 */
const renamingId = ref<string | null>(null);
const draftName = ref('');
const renameInput = ref<HTMLInputElement | null>(null);

/**
 * Set by the field itself rather than by a `ref="…"` attribute.
 *
 * A named ref inside a `v-for` collects an *array*, even when only one row
 * renders the element — and an array is truthy, so `renameInput.value?.select()`
 * sailed past the optional chain and threw on every rename.
 */
const setRenameInput = (el: unknown) => {
  renameInput.value = (el as HTMLInputElement | null) ?? null;
};

const startRename = async (id: string, current: string) => {
  renamingId.value = id;
  draftName.value = current;
  await nextTick();
  renameInput.value?.select();
};

const cancelRename = () => {
  renamingId.value = null;
  draftName.value = '';
};

const commitRename = (id: string) => {
  const trimmed = draftName.value.trim();
  // An empty name would leave a row with nothing to click on, and an unchanged
  // one is a write for no reason.
  if (trimmed) emit('rename-filter', id, trimmed);
  cancelRename();
};

const emit = defineEmits<{
  (e: 'update:activeCategory', value: string): void;
  (e: 'create-project'): void;
  (e: 'delete-filter', id: string): void;
  (e: 'rename-filter', id: string, name: string): void;
  (e: 'close-mobile'): void;
}>();

const selectCategory = (cat: string) => {
  emit('update:activeCategory', cat);
  if (props.variant === 'mobile') emit('close-mobile');
};
</script>

<template>
  <!-- DESKTOP SIDEBAR -->
  <div v-if="variant === 'desktop'" class="w-64 border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50/50 dark:bg-[#1a1a1a]/50 flex flex-col pt-10 shrink-0 hidden md:flex">
      <div class="flex flex-col px-3 space-y-1">
          <button @click="selectCategory('all')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-white dark:bg-[#2c2c2c] text-black dark:text-white shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Inbox class="w-4 h-4 mr-3" />{{ $t('task.all_tasks') }}</div>
              <span class="text-xs bg-gray-200 dark:bg-[#333] px-1.5 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
          </button>
          <button @click="selectCategory('today')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-white dark:bg-[#2c2c2c] text-blue-600 dark:text-blue-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Sun class="w-4 h-4 mr-3" />{{ $t('task.today') }}</div>
              <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-1.5 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
          </button>
          <button @click="selectCategory('upcoming')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-white dark:bg-[#2c2c2c] text-red-600 dark:text-red-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Calendar class="w-4 h-4 mr-3" />{{ $t('task.upcoming') }}</div>
              <span class="text-xs bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
          </button>
          <button @click="selectCategory('someday')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-white dark:bg-[#2c2c2c] text-yellow-600 dark:text-yellow-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Coffee class="w-4 h-4 mr-3" />{{ $t('task.someday') }}</div>
              <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-1.5 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
          </button>
          <button @click="selectCategory('transferred')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-white dark:bg-[#2c2c2c] text-slate-600 dark:text-slate-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Send class="w-4 h-4 mr-3" />{{ $t('task.transferred') }}</div>
              <span class="text-xs bg-slate-200 dark:bg-slate-700 px-1.5 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
          </button>
          
          <div class="pt-4 pb-1 px-3 flex items-center justify-between group">
              <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.projects') }}</span>
              <button @click="emit('create-project')" class="text-gray-400 hover:text-indigo-500 opacity-0 group-hover:opacity-100 transition-opacity" :title="$t('task.new_project')">
                  <Plus class="w-3.5 h-3.5"/>
              </button>
          </div>
          <button v-for="proj in projects" :key="proj.id" @click="selectCategory('project:' + proj.id)" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer group" :class="activeCategory === 'project:' + proj.id ? 'bg-white dark:bg-[#2c2c2c] text-indigo-600 dark:text-indigo-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center truncate">
                  <svg class="w-4 h-4 mr-3 shrink-0" :class="activeCategory === 'project:' + proj.id ? 'text-indigo-500' : 'text-gray-400 group-hover:text-indigo-400'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                  <span class="truncate">{{ proj.title }}</span>
              </div>
          </button>

          <!--
            Saved searches, below the projects. A project is a place work
            lives; a search is a way of looking at it. One list would suggest
            they are the same kind of thing.
          -->
          <!--
            The heading shows even with nothing under it. Hiding the section
            until the first search is saved means the only way to find out that
            searches can be saved is to already know.
          -->
          <div class="pt-4 pb-1 px-3">
            <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.filters') }}</span>
          </div>
          <p v-if="!props.filters?.length" class="px-3 pb-1 text-[11px] leading-relaxed text-gray-400 dark:text-gray-600">
            {{ $t('task.filter_empty_hint') }}
          </p>
          <template v-if="props.filters?.length">
            <div v-for="f in props.filters" :key="f.id" class="group/f flex items-center">
              <template v-if="renamingId === f.id">
                <div class="flex-1 min-w-0 flex items-center px-3 py-1.5 rounded-lg border border-blue-300 dark:border-blue-800">
                  <Filter class="w-4 h-4 mr-3 shrink-0 text-blue-500" />
                  <input
                    :ref="setRenameInput"
                    v-model="draftName"
                    @keydown.enter.prevent="commitRename(f.id)"
                    @keydown.escape.prevent="cancelRename"
                    @blur="commitRename(f.id)"
                    type="text"
                    class="w-full min-w-0 bg-transparent border-none outline-none text-sm text-[#1c1c1e] dark:text-[#f4f4f5]"
                    :aria-label="$t('task.filter_rename')"
                  />
                </div>
              </template>

              <template v-else>
                <button @click="selectCategory('filter:' + f.id)" class="flex-1 min-w-0 flex items-center px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'filter:' + f.id ? 'bg-white dark:bg-[#2c2c2c] text-blue-600 dark:text-blue-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <Filter class="w-4 h-4 mr-3 shrink-0" />
                  <span class="truncate">{{ f.name }}</span>
                </button>
                <button @click.stop="startRename(f.id, f.name)" class="shrink-0 p-1 rounded-md text-gray-300 hover:text-blue-500 opacity-0 group-hover/f:opacity-100 focus:opacity-100 transition-opacity cursor-pointer" :title="$t('task.filter_rename')" :aria-label="$t('task.filter_rename')">
                  <Pencil class="w-3.5 h-3.5" />
                </button>
                <button @click.stop="emit('delete-filter', f.id)" class="shrink-0 p-1 mr-1 rounded-md text-gray-300 hover:text-red-500 opacity-0 group-hover/f:opacity-100 focus:opacity-100 transition-opacity cursor-pointer" :title="$t('task.filter_delete')" :aria-label="$t('task.filter_delete')">
                  <X class="w-3.5 h-3.5" />
                </button>
              </template>
            </div>
          </template>

      </div>
  </div>

  <!-- MOBILE SIDEBAR OVERLAY -->
  <div v-if="variant === 'mobile' && isMobileOpen" class="fixed inset-0 z-[120] md:hidden flex">
      <!-- Backdrop -->
      <div class="absolute inset-0 bg-black/20 dark:bg-black/60 backdrop-blur-sm transition-opacity" @click="emit('close-mobile')"></div>
      
      <!-- Sidebar Panel -->
      <div class="relative w-[75%] max-w-sm h-full bg-[#fdfdfc] dark:bg-[#1e1e1e] shadow-2xl flex flex-col transform transition-transform duration-300" style="padding-top: max(env(safe-area-inset-top), 20px);">
          <!-- Header with Close Button -->
          <div class="flex items-center justify-between px-5 pb-4 border-b border-gray-100 dark:border-[#2c2c2c] shrink-0">
              <h2 class="text-xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('task.views') }}</h2>
              <button @click="emit('close-mobile')" class="p-2 -mr-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer" :aria-label="$t('task.a11y_close_sidebar')">
                  <X class="w-5 h-5" />
              </button>
          </div>
          
          <!-- Menu Items -->
          <div class="flex-1 overflow-y-auto px-3 py-6 flex flex-col space-y-1.5">
              <button @click="selectCategory('all')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-black/5 dark:bg-white/10 text-black dark:text-white font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Inbox class="w-5 h-5 mr-3" />{{ $t('task.all_tasks') }}</div>
                  <span class="text-xs bg-gray-200 dark:bg-[#333] px-2 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
              </button>
              <button @click="selectCategory('today')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Sun class="w-5 h-5 mr-3" />{{ $t('task.today') }}</div>
                  <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-2 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
              </button>
              <button @click="selectCategory('upcoming')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Calendar class="w-5 h-5 mr-3" />{{ $t('task.upcoming') }}</div>
                  <span class="text-xs bg-red-100 dark:bg-red-900/30 px-2 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
              </button>
              <button @click="selectCategory('someday')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-yellow-50 dark:bg-yellow-900/20 text-yellow-600 dark:text-yellow-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Coffee class="w-5 h-5 mr-3" />{{ $t('task.someday') }}</div>
                  <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-2 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
              </button>
              <button @click="selectCategory('transferred')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Send class="w-5 h-5 mr-3" />{{ $t('task.transferred') }}</div>
                  <span class="text-xs bg-slate-200 dark:bg-slate-700 px-2 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
              </button>
              
              <div class="pt-4 pb-1 px-3 flex items-center justify-between">
                  <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.projects') }}</span>
                  <button @click="emit('create-project')" class="text-gray-400 hover:text-indigo-500" :title="$t('task.new_project')">
                      <Plus class="w-4 h-4"/>
                  </button>
              </div>
              <button v-for="proj in projects" :key="proj.id" @click="selectCategory('project:' + proj.id)" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'project:' + proj.id ? 'bg-indigo-50 dark:bg-indigo-900/20 text-indigo-600 dark:text-indigo-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center truncate">
                      <svg class="w-5 h-5 mr-3 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                      <span class="truncate">{{ proj.title }}</span>
                  </div>
              </button>

              <div class="pt-4 pb-1 px-3">
                <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.filters') }}</span>
              </div>
              <p v-if="!props.filters?.length" class="px-3 pb-1 text-[11px] leading-relaxed text-gray-400 dark:text-gray-600">
                {{ $t('task.filter_empty_hint') }}
              </p>
              <template v-if="props.filters?.length">
                <!--
                  The actions sit open here rather than on hover: a phone has
                  no hover, and a control that only appears on one is a control
                  that does not exist on a phone.
                -->
                <div v-for="f in props.filters" :key="f.id" class="flex items-center">
                  <div v-if="renamingId === f.id" class="flex-1 min-w-0 flex items-center px-3 py-2.5 rounded-xl border border-blue-300 dark:border-blue-800">
                    <Filter class="w-5 h-5 mr-3 shrink-0 text-blue-500" />
                    <input
                      :ref="setRenameInput"
                      v-model="draftName"
                      @keydown.enter.prevent="commitRename(f.id)"
                      @keydown.escape.prevent="cancelRename"
                      type="text"
                      class="w-full min-w-0 bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5]"
                      :aria-label="$t('task.filter_rename')"
                    />
                  </div>
                  <template v-else>
                    <button @click="selectCategory('filter:' + f.id)" class="flex-1 min-w-0 flex items-center px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'filter:' + f.id ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                      <Filter class="w-5 h-5 mr-3 shrink-0" />
                      <span class="truncate">{{ f.name }}</span>
                    </button>
                    <button @click.stop="startRename(f.id, f.name)" class="shrink-0 p-2 rounded-lg text-gray-400 hover:text-blue-500 transition-colors cursor-pointer" :aria-label="$t('task.filter_rename')">
                      <Pencil class="w-4 h-4" />
                    </button>
                    <button @click.stop="emit('delete-filter', f.id)" class="shrink-0 p-2 rounded-lg text-gray-400 hover:text-red-500 transition-colors cursor-pointer" :aria-label="$t('task.filter_delete')">
                      <X class="w-4 h-4" />
                    </button>
                  </template>
                </div>
              </template>

          </div>
      </div>
  </div>
</template>
