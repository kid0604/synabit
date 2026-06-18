<script setup lang="ts">
import { CalendarDays, Plus, Settings, ChevronDown, Link, FileText, Palette, File, Unlink } from 'lucide-vue-next';
import { type TaskMetadata, isOverdue } from '../types';

defineProps<{
  activeProject: any;
  activeProjectTab: string;
  activeCategoryTasks: TaskMetadata[];
  projectProgress: number;
  projectBudget: string | null;
  projectSpent: string;
  displayCustomFields: { key: string; val: any }[];
  linkedResources: any[];
  isLinkingResource: boolean;
  showAddResourceMenu: boolean;
  showEmptyAddMenu: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:activeProjectTab', value: string): void;
  (e: 'edit-project'): void;
  (e: 'show-tx-modal'): void;
  (e: 'create-note'): void;
  (e: 'create-whiteboard'): void;
  (e: 'open-link-picker'): void;
  (e: 'unlink-resource', node: any): void;
  (e: 'open-node', id: string, type: string): void;
  (e: 'update:showAddResourceMenu', value: boolean): void;
  (e: 'update:showEmptyAddMenu', value: boolean): void;
}>();
</script>

<template>
  <div v-if="activeProject" class="mb-6 mt-2 space-y-6 relative group">
      <!-- Tabs Navigation -->
      <div class="flex items-center justify-between border-b border-gray-200 dark:border-gray-800 px-2">
          <div class="flex items-center gap-6">
              <button @click="emit('update:activeProjectTab', 'overview')" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'overview' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                  {{ $t('task.overview') }}
                  <div v-if="activeProjectTab === 'overview'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
              </button>
              <button @click="emit('update:activeProjectTab', 'tasks')" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'tasks' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                  {{ $t('task.tasks') }}
                  <div v-if="activeProjectTab === 'tasks'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
              </button>
              <button @click="emit('update:activeProjectTab', 'resources')" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'resources' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                  {{ $t('task.resources') }}
                  <div v-if="activeProjectTab === 'resources'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
              </button>
          </div>
          <button @click="emit('edit-project')" class="pb-3 text-gray-400 hover:text-indigo-500 transition-colors cursor-pointer" :title="$t('task.project_settings')">
              <Settings class="w-4 h-4" />
          </button>
      </div>

      <div v-if="activeProjectTab === 'overview'" class="space-y-6 animate-in fade-in slide-in-from-bottom-2 duration-300">
          
          <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
              <!-- Project Description Card -->
              <div class="md:col-span-1 bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm flex flex-col">
                  <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-3">{{ $t('task.project_description') }}</h3>
                  <div v-if="activeProject.content" class="text-sm text-gray-600 dark:text-gray-400 prose prose-sm dark:prose-invert max-w-none mb-4 line-clamp-3">
                      <div v-html="activeProject.content"></div>
                  </div>
                  <div v-else class="text-sm text-gray-400 italic mb-4">{{ $t('task.no_description') }}</div>
                  
                  <div class="mt-auto space-y-3 pt-3 border-t border-gray-50 dark:border-[#2c2c2c]">
                      <div class="flex items-center justify-between">
                          <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">{{ $t('task.status') }}</div>
                          <span class="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium capitalize" 
                              :class="{
                                  'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400': activeProject.status === 'active',
                                  'bg-green-50 text-green-700 dark:bg-green-900/30 dark:text-green-400': activeProject.status === 'completed',
                                  'bg-yellow-50 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400': activeProject.status === 'on_hold'
                              }">
                              {{ activeProject.status.replace('_', ' ') }}
                          </span>
                      </div>
                      <div v-if="activeProject.tags?.length > 0" class="flex items-center justify-between">
                          <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">{{ $t('task.tags') }}</div>
                          <div class="flex flex-wrap items-center gap-1 justify-end">
                              <span v-for="tag in activeProject.tags.slice(0,3)" :key="tag" class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 dark:bg-[#2c2c2c] text-gray-600 dark:text-gray-400">
                                  #{{ tag }}
                              </span>
                              <span v-if="activeProject.tags.length > 3" class="text-[10px] text-gray-400">+{{activeProject.tags.length - 3}}</span>
                          </div>
                      </div>
                      <div class="flex items-center justify-between">
                          <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">{{ $t('task.created') }}</div>
                          <div class="text-xs text-gray-700 dark:text-gray-300">{{ activeProject.created_at ? activeProject.created_at.substring(0, 10) : '--' }}</div>
                      </div>
                      <div class="flex items-center justify-between">
                          <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">{{ $t('task.updated') }}</div>
                          <div class="text-xs text-gray-700 dark:text-gray-300">{{ activeProject.updated_at ? activeProject.updated_at.substring(0, 10) : '--' }}</div>
                      </div>
                  </div>
              </div>

              <!-- Time & Budget Card -->
              <div class="bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm hover:shadow-md transition-shadow">
                  <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-4">{{ $t('task.time_and_budget') }}</h3>
                  
                  <div class="space-y-5">
                      <div class="grid grid-cols-2 gap-4">
                          <div>
                              <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">{{ $t('task.start_date') }}</div>
                              <div class="text-sm font-semibold text-gray-900 dark:text-gray-100 flex items-center">
                                  <CalendarDays class="w-3.5 h-3.5 mr-1.5 text-gray-400" />
                                  {{ activeProject.start_date || '--/--/----' }}
                              </div>
                          </div>
                          <div>
                              <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">{{ $t('task.end_date') }}</div>
                              <div class="text-sm font-semibold text-red-500 flex items-center">
                                  <CalendarDays class="w-3.5 h-3.5 mr-1.5" />
                                  {{ activeProject.due_date || '--/--/----' }}
                              </div>
                          </div>
                      </div>
                      
                      <div class="pt-4 border-t border-gray-50 dark:border-[#2c2c2c]">
                          <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">{{ $t('task.budget') }}</div>
                          <div class="text-xl font-bold text-gray-900 dark:text-gray-100">
                              {{ projectBudget || $t('task.not_set') }}
                          </div>
                      </div>
                      
                      <div v-if="projectSpent">
                          <div class="flex items-center justify-between mb-1">
                              <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">{{ $t('task.spent') }}</div>
                              <button @click="emit('show-tx-modal')" class="text-[10px] flex items-center bg-gray-100 hover:bg-gray-200 dark:bg-[#333] dark:hover:bg-[#444] text-gray-600 dark:text-gray-300 px-1.5 py-0.5 rounded transition-colors" :title="$t('task.log_expense')">
                                  <Plus class="w-3 h-3 mr-0.5" /> {{ $t('task.add_btn') }}
                              </button>
                          </div>
                          <div class="text-lg font-semibold text-orange-500">
                              {{ projectSpent }}
                          </div>
                      </div>
                      
                      <div v-if="displayCustomFields.length > 0">
                          <div v-for="field in displayCustomFields" :key="field.key">
                              <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-0.5 mt-3">{{ field.key }}</div>
                              <div class="text-sm font-medium text-gray-800 dark:text-gray-200">{{ field.val }}</div>
                          </div>
                      </div>
                  </div>
              </div>

              <!-- Progress & Task Summary Card -->
              <div class="bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm hover:shadow-md transition-shadow flex flex-col">
                   <div class="mb-6">
                      <div class="flex items-center justify-between mb-2">
                          <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100">{{ $t('task.project_progress') }}</h3>
                          <span class="text-xs font-bold text-indigo-600 dark:text-indigo-400 bg-indigo-50 dark:bg-indigo-900/30 px-2 py-0.5 rounded-full">{{ projectProgress }}%</span>
                      </div>
                      <div class="w-full bg-gray-100 dark:bg-gray-800 rounded-full h-2.5 overflow-hidden">
                          <div class="bg-gradient-to-r from-blue-400 to-indigo-500 h-2.5 rounded-full transition-all duration-500" :style="{ width: projectProgress + '%' }"></div>
                      </div>
                  </div>
                  
                  <div class="flex-1 border-t border-gray-50 dark:border-[#2c2c2c] pt-4">
                      <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-4">{{ $t('task.task_summary') }}</h3>
                      <div class="grid grid-cols-3 gap-3">
                          <!-- Total -->
                          <div class="bg-gray-50 dark:bg-[#252525] rounded-xl p-3">
                              <div class="text-xl font-bold text-gray-900 dark:text-gray-100">{{ activeCategoryTasks.length }}</div>
                              <div class="text-[9px] font-medium text-gray-500 uppercase tracking-wider mt-1">{{ $t('task.total') }}</div>
                          </div>
                          <!-- In Progress -->
                          <div class="bg-blue-50 dark:bg-blue-900/20 rounded-xl p-3">
                              <div class="text-xl font-bold text-blue-600 dark:text-blue-400">{{ activeCategoryTasks.filter(t => t.status === 'in_progress').length }}</div>
                              <div class="text-[9px] font-medium text-blue-600/70 dark:text-blue-400/70 uppercase tracking-wider mt-1">{{ $t('task.doing') }}</div>
                          </div>
                          <!-- To Do -->
                          <div class="bg-orange-50 dark:bg-orange-900/20 rounded-xl p-3">
                              <div class="text-xl font-bold text-orange-600 dark:text-orange-400">{{ activeCategoryTasks.filter(t => t.status === 'todo').length }}</div>
                              <div class="text-[9px] font-medium text-orange-600/70 dark:text-orange-400/70 uppercase tracking-wider mt-1">{{ $t('task.to_do') }}</div>
                          </div>
                          <!-- Backlog -->
                          <div class="bg-purple-50 dark:bg-purple-900/20 rounded-xl p-3">
                              <div class="text-xl font-bold text-purple-600 dark:text-purple-400">{{ activeCategoryTasks.filter(t => t.status === 'backlog').length }}</div>
                              <div class="text-[9px] font-medium text-purple-600/70 dark:text-purple-400/70 uppercase tracking-wider mt-1">{{ $t('task.backlog') }}</div>
                          </div>
                          <!-- Completed -->
                          <div class="bg-green-50 dark:bg-green-900/20 rounded-xl p-3">
                              <div class="text-xl font-bold text-green-600 dark:text-green-400">{{ activeCategoryTasks.filter(t => t.status === 'done').length }}</div>
                              <div class="text-[9px] font-medium text-green-600/70 dark:text-green-400/70 uppercase tracking-wider mt-1">{{ $t('task.done') }}</div>
                          </div>
                          <!-- Overdue -->
                          <div class="bg-red-50 dark:bg-red-900/20 rounded-xl p-3">
                              <div class="text-xl font-bold text-red-600 dark:text-red-400">{{ activeCategoryTasks.filter(t => isOverdue(t)).length }}</div>
                              <div class="text-[9px] font-medium text-red-600/70 dark:text-red-400/70 uppercase tracking-wider mt-1">{{ $t('task.overdue') }}</div>
                          </div>
                      </div>
                  </div>
              </div>
          </div>
      </div>

      <!-- RESOURCES TAB -->
      <div v-if="activeProjectTab === 'resources'" class="animate-in fade-in slide-in-from-bottom-2 duration-300">
          <div class="flex justify-end gap-2 mb-4 relative">
              <button @click="emit('update:showAddResourceMenu', !showAddResourceMenu)" :disabled="isLinkingResource" class="px-4 py-2 flex items-center gap-2 rounded-lg bg-indigo-50 text-indigo-600 dark:bg-indigo-900/30 dark:text-indigo-400 hover:bg-indigo-100 dark:hover:bg-indigo-900/50 transition-colors text-sm font-medium cursor-pointer">
                  <Plus class="w-4 h-4" />
                  {{ $t('task.add_resource') }}
                  <ChevronDown class="w-4 h-4 ml-1" />
              </button>
              
              <!-- Dropdown Menu -->
              <div v-if="showAddResourceMenu" class="absolute top-full right-0 mt-2 w-56 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-lg border border-gray-100 dark:border-[#2c2c2c] overflow-hidden z-20">
                  <div class="p-1">
                      <button @click="emit('create-note'); emit('update:showAddResourceMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                          <FileText class="w-4 h-4 text-blue-500" />
                          {{ $t('task.new_note') }}
                      </button>
                      <button @click="emit('create-whiteboard'); emit('update:showAddResourceMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                          <Palette class="w-4 h-4 text-purple-500" />
                          {{ $t('task.new_whiteboard') }}
                      </button>
                      <div class="h-px bg-gray-100 dark:bg-[#2c2c2c] my-1"></div>
                      <button @click="emit('open-link-picker'); emit('update:showAddResourceMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                          <Link class="w-4 h-4 text-gray-400" />
                          {{ $t('task.link_existing') }}
                      </button>
                  </div>
              </div>
          </div>

          <!-- Close dropdown when clicking outside -->
          <div v-if="showAddResourceMenu" @click="emit('update:showAddResourceMenu', false)" class="fixed inset-0 z-10"></div>

          <div v-if="linkedResources.length > 0">
              <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
                  <div v-for="node in linkedResources" :key="node.id" @click="emit('open-node', node.id, node.node_type || 'note')" 
                       class="bg-white dark:bg-[#1a1a1a] border border-gray-200 dark:border-[#2c2c2c] rounded-xl p-4 shadow-sm hover:shadow-md cursor-pointer transition-all group"
                       :class="[
                           node.node_type === 'whiteboard' ? 'hover:border-purple-300 dark:hover:border-purple-700' :
                           node.node_type === 'file' ? 'hover:border-emerald-300 dark:hover:border-emerald-700' :
                           'hover:border-blue-300 dark:hover:border-blue-700'
                       ]">
                       <div class="font-medium text-[15px] text-[#1c1c1e] dark:text-[#f4f4f5] mb-2 flex items-center justify-between">
                          <div class="flex items-center min-w-0 pr-2">
                              <Palette v-if="node.node_type === 'whiteboard'" class="w-4 h-4 mr-2 text-purple-400 shrink-0" />
                              <File v-else-if="node.node_type === 'file'" class="w-4 h-4 mr-2 text-emerald-400 shrink-0" />
                              <FileText v-else class="w-4 h-4 mr-2 text-blue-400 shrink-0" />
                              <span class="truncate">{{ node.title || (node.node_type === 'whiteboard' ? $t('task.untitled_whiteboard') : node.node_type === 'file' ? $t('task.unnamed_file') : $t('task.untitled_note')) }}</span>
                          </div>
                          <button @click.stop="emit('unlink-resource', node)" :title="$t('task.unlink')" class="opacity-0 group-hover:opacity-100 p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-md text-gray-400 hover:text-red-500 transition-all shrink-0">
                              <Unlink class="w-3.5 h-3.5" />
                          </button>
                       </div>
                      <div v-if="node.node_type === 'file'" class="text-xs text-gray-500 mt-2 font-mono truncate">
                          {{ node.id }}
                      </div>
                      <div v-else class="text-xs text-gray-500 line-clamp-2 leading-relaxed">
                          {{ node.content ? node.content.replace(/<[^>]+>/g, '').substring(0, 80) + '...' : $t('task.empty') + ' ' + (node.node_type === 'whiteboard' ? $t('task.whiteboard_lower') : $t('task.note_lower')) }}
                      </div>
                  </div>
              </div>
          </div>
          <div v-else class="flex flex-col items-center justify-center h-48 opacity-80 bg-white/50 dark:bg-black/20 rounded-2xl border border-dashed border-gray-200 dark:border-gray-800">
              <div class="flex gap-2 mb-3">
                  <FileText class="w-10 h-10 text-gray-300" />
                  <Palette class="w-10 h-10 text-gray-300" />
              </div>
              <p class="text-sm font-medium text-gray-500 mb-4">{{ $t('task.no_resources') }}</p>
              <div class="flex gap-3 relative">
                  <button @click="emit('update:showEmptyAddMenu', !showEmptyAddMenu)" class="px-4 py-2 bg-indigo-500 text-white rounded-lg text-sm font-medium hover:bg-indigo-600 transition-colors flex items-center gap-2 cursor-pointer">
                      {{ $t('task.add_resource') }}
                      <ChevronDown class="w-4 h-4" />
                  </button>
                  
                  <!-- Dropdown Menu for empty state -->
                  <div v-if="showEmptyAddMenu" class="absolute top-full left-1/2 -translate-x-1/2 mt-2 w-56 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-lg border border-gray-100 dark:border-[#2c2c2c] overflow-hidden z-20">
                      <div class="p-1">
                          <button @click="emit('create-note'); emit('update:showEmptyAddMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                              <FileText class="w-4 h-4 text-blue-500" />
                              {{ $t('task.new_note') }}
                          </button>
                          <button @click="emit('create-whiteboard'); emit('update:showEmptyAddMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                              <Palette class="w-4 h-4 text-purple-500" />
                              {{ $t('task.new_whiteboard') }}
                          </button>
                          <div class="h-px bg-gray-100 dark:bg-[#2c2c2c] my-1"></div>
                          <button @click="emit('open-link-picker'); emit('update:showEmptyAddMenu', false)" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                              <Link class="w-4 h-4 text-gray-400" />
                              {{ $t('task.link_existing') }}
                          </button>
                      </div>
                  </div>
              </div>
              
              <!-- Close dropdown when clicking outside -->
              <div v-if="showEmptyAddMenu" @click="emit('update:showEmptyAddMenu', false)" class="fixed inset-0 z-10"></div>
          </div>
      </div>
  </div>
</template>
