<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';
import { X, Calendar, CheckCircle2, Trash2, Tag, Activity, PlusCircle, DollarSign } from 'lucide-vue-next';
import TiptapEditor from '../note/TiptapEditor.vue';

const props = defineProps<{
    project: any;
    vaultPath: string;
    dynamicSpent?: number;
}>();

const emit = defineEmits(['save', 'close', 'delete']);

const editingProject = ref({
    title: props.project?.title || '',
    content: props.project?.content || '',
    due_date: props.project?.due_date || '',
    start_date: props.project?.start_date || '',
    status: props.project?.status || 'active',
    tags: Array.isArray(props.project?.tags) ? [...props.project.tags] : (props.project?.tags ? [props.project.tags] : [])
});

const getCaseInsensitiveField = (key: string, defaultValue: string = '') => {
    if (!props.project?.custom_fields) return defaultValue;
    const lowerKey = key.toLowerCase();
    const foundKey = Object.keys(props.project.custom_fields).find(k => k.toLowerCase() === lowerKey);
    return foundKey ? props.project.custom_fields[foundKey] : defaultValue;
};

const wipLimitInput = ref(getCaseInsensitiveField('wip_limit', '5'));

const formatNumber = (val: string | number) => {
    if (!val) return '';
    const num = String(val).replace(/[^0-9.]/g, '');
    const parts = num.split('.');
    parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ',');
    return parts.join('.');
};

const rawBudget = ref(String(getCaseInsensitiveField('budget', '')));
const budgetInput = ref(formatNumber(rawBudget.value));
const rawCurrency = ref(getCaseInsensitiveField('currency', 'VND'));

const handleBudgetInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    const val = target.value;
    const clean = val.replace(/[^0-9.]/g, '');
    rawBudget.value = clean;
    const formatted = formatNumber(clean);
    budgetInput.value = formatted;
    
    // Force DOM sync to strip invalid chars visually
    if (val !== formatted) {
        target.value = formatted;
    }
};

const spentDisplay = computed(() => formatNumber(props.dynamicSpent ?? 0));

const tagInput = ref('');

const addTag = (event?: Event) => {
    if (event) event.preventDefault();
    const val = tagInput.value.trim();
    if (val && !editingProject.value.tags.includes(val)) {
        editingProject.value.tags.push(val);
    }
    tagInput.value = '';
};

const removeTag = (index: number) => {
    editingProject.value.tags.splice(index, 1);
};

const activeDropdown = ref<string | null>(null);
const confirmDeleteIndex = ref<number | null>(null);
const showDeleteConfirm = ref(false);

const standardKeys = ['id', 'title', 'content', 'status', 'start_date', 'due_date', 'color', 'tags', 'created_at', 'updated_at', 'type', 'node_type', 'path', 'timestamp', 'wip_limit', 'budget', 'spent', 'currency'];

const customProperties = ref(
    Object.entries(props.project?.custom_fields || {})
        .filter(([key]) => !standardKeys.includes(key.toLowerCase()))
        .map(([key, value]) => ({
            key,
            value: String(value)
        }))
);

const addCustomProperty = () => {
    customProperties.value.push({ key: '', value: '' });
};

const removeCustomProperty = (index: number) => {
    customProperties.value.splice(index, 1);
    confirmDeleteIndex.value = null;
};

const handleGlobalClick = () => {
    activeDropdown.value = null;
};

onMounted(() => {
    document.addEventListener('click', handleGlobalClick);
});

onUnmounted(() => {
    document.removeEventListener('click', handleGlobalClick);
});

const save = () => {
    const custom_fields: Record<string, string> = {};
    for (const prop of customProperties.value) {
        if (prop.key.trim()) {
            custom_fields[prop.key.trim()] = prop.value.trim();
        }
    }
    
    if (wipLimitInput.value) {
        custom_fields['wip_limit'] = String(wipLimitInput.value);
    }
    if (rawBudget.value) {
        custom_fields['budget'] = rawBudget.value;
    }
    custom_fields['currency'] = rawCurrency.value;
    
    emit('save', {
        ...editingProject.value,
        tags: [...editingProject.value.tags],
        custom_fields
    });
};

const close = () => {
    emit('close');
};

const handleBackgroundClick = () => {
    save();
};
</script>

<template>
  <div class="fixed inset-0 z-[110] flex items-center justify-center md:p-4 bg-black/10 dark:bg-black/40 backdrop-blur-[2px]" @mousedown.self="handleBackgroundClick">
      <div class="w-full h-full md:h-auto md:max-w-lg bg-white dark:bg-[#1e1e1e] md:rounded-2xl shadow-none md:shadow-[0_20px_40px_rgba(0,0,0,0.1)] md:dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border-none md:border md:border-gray-100 md:dark:border-[#2c2c2c] overflow-hidden flex flex-col" @mousedown.stop>
          
          <div class="flex justify-between items-center px-5 pb-4 md:hidden shrink-0 border-b border-gray-100 dark:border-[#2c2c2c]" style="padding-top: max(env(safe-area-inset-top), 36px);">
              <h3 class="font-semibold text-lg text-[#1c1c1e] dark:text-[#f4f4f5]">Edit Project</h3>
              <button @click="handleBackgroundClick" class="p-2 -mr-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]">
                  <X class="w-4 h-4" />
              </button>
          </div>

          <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
              <div class="flex items-start gap-4 mb-4">
                   <input 
                       v-model="editingProject.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 focus:ring-0 p-0 leading-snug"
                       placeholder="Project Title"
                   />
              </div>
              
              <!-- Standard Properties -->
              <div class="mb-1.5 space-y-1.5">
                  <!-- Status -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Activity class="w-3 h-3 mr-2 opacity-70"/> Status</div>
                      <select v-model="editingProject.status" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-[#1c1c1e] dark:text-[#f4f4f5] font-medium appearance-none cursor-pointer">
                          <option value="active">Active</option>
                          <option value="on_hold">On Hold</option>
                          <option value="completed">Completed</option>
                      </select>
                      <div class="w-[22px]"></div> <!-- Spacer to align with custom properties X button -->
                  </div>
                  
                  <!-- Dates -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Calendar class="w-3 h-3 mr-2 opacity-70"/> Dates</div>
                      <div class="flex-1 flex items-center gap-1 bg-gray-50 dark:bg-[#2c2c2c] rounded px-1.5 border border-transparent focus-within:border-gray-200 dark:focus-within:border-gray-700 transition-colors">
                          <input type="date" v-model="editingProject.start_date" class="w-full text-xs bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] py-1.5 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                          <span class="text-gray-400 text-xs px-1">→</span>
                          <input type="date" v-model="editingProject.due_date" class="w-full text-xs bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] py-1.5 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" />
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- WIP Limit -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Activity class="w-3 h-3 mr-2 opacity-70"/> WIP Limit</div>
                      <input type="number" min="1" v-model="wipLimitInput" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-[#1c1c1e] dark:text-[#f4f4f5] font-medium" placeholder="e.g. 5" />
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- Budget -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><DollarSign class="w-3 h-3 mr-2 opacity-70"/> Budget</div>
                      <div class="flex-1 flex items-center gap-1">
                          <input type="text" :value="budgetInput" @input="handleBudgetInput" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-[#1c1c1e] dark:text-[#f4f4f5] font-medium" placeholder="e.g. 10,000,000" />
                          <select v-model="rawCurrency" class="w-[70px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-gray-600 dark:text-gray-300 font-medium appearance-none cursor-pointer text-center">
                              <option value="VND">VND</option>
                              <option value="USD">USD</option>
                              <option value="EUR">EUR</option>
                              <option value="JPY">JPY</option>
                          </select>
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- Spent (Readonly) -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><DollarSign class="w-3 h-3 mr-2 opacity-70"/> Spent</div>
                      <div class="flex-1 text-xs bg-gray-100 dark:bg-[#222] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium cursor-not-allowed flex items-center justify-between">
                          <span>{{ spentDisplay }}</span>
                          <span class="text-[10px] font-bold text-gray-400">{{ rawCurrency }}</span>
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- Tags -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Tag class="w-3 h-3 mr-2 opacity-70"/> Tags</div>
                      <div class="flex-1 flex flex-wrap items-center gap-1.5 bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus-within:border-gray-200 dark:focus-within:border-gray-700 rounded p-1 transition-colors min-h-[32px]">
                          <span v-for="(tag, index) in editingProject.tags" :key="index" class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm bg-gray-200 dark:bg-[#3d3d3d] text-gray-700 dark:text-gray-300 text-[11px] font-medium">
                              #{{ tag }}
                              <button @click.prevent="removeTag(index)" class="hover:text-red-500 focus:outline-none"><X class="w-3 h-3"/></button>
                          </span>
                          <input v-model="tagInput" @keydown.enter.prevent="addTag" @blur="addTag" placeholder="Add tag..." class="flex-1 min-w-[80px] text-xs bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 py-0.5 px-1" />
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
              </div>
              
              <!-- Custom Properties -->
              <div v-if="customProperties.length > 0" class="mb-1.5 space-y-1.5">
                  <div v-for="(prop, index) in customProperties" :key="index" class="flex items-center gap-2 group relative">
                      <input v-model="prop.key" placeholder="Property name..." class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-gray-500 dark:text-gray-400 font-medium placeholder-gray-300" />
                      <input v-model="prop.value" placeholder="Value..." class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300" />
                      
                      <div v-if="confirmDeleteIndex === index" class="absolute right-0 top-0 bottom-0 bg-white/90 dark:bg-[#1e1e1e]/90 flex items-center justify-end px-2 gap-2 rounded z-10 w-full backdrop-blur-[2px]">
                          <span class="text-[11px] text-red-500 font-medium">Delete this property?</span>
                          <button @click.stop="removeCustomProperty(index)" class="text-[10px] bg-red-500 text-white px-2 py-1 rounded font-medium hover:bg-red-600 transition-colors">Yes</button>
                          <button @click.stop="confirmDeleteIndex = null" class="text-[10px] bg-gray-200 dark:bg-[#333] text-gray-700 dark:text-gray-300 px-2 py-1 rounded font-medium hover:bg-gray-300 dark:hover:bg-[#444] transition-colors">Cancel</button>
                      </div>
                      <button v-else @click="confirmDeleteIndex = index" class="p-1 text-gray-400 hover:text-red-500 rounded transition-colors"><X class="w-3.5 h-3.5" /></button>
                  </div>
              </div>
              <div class="mb-4">
                  <button @click="addCustomProperty" class="text-[11px] font-medium text-gray-400 hover:text-indigo-500 flex items-center transition-colors">
                      <PlusCircle class="w-3 h-3 mr-1" /> Add property
                  </button>
              </div>
              
              <div class="mb-4 flex-1 flex flex-col min-h-[100px] max-h-[300px] overflow-y-auto overflow-x-hidden custom-scrollbar border-t border-gray-100 dark:border-[#2c2c2c] pt-4">
                  <TiptapEditor 
                       v-model="editingProject.content" 
                       :vaultPath="props.vaultPath || ''"
                       :minHeightClass="'min-h-[100px]'"
                       class="w-full flex-1"
                       placeholder="Add a description or notes for this project..."
                  />
              </div>
          </div>
          
          <div v-if="!project.isNew" class="px-5 pt-3 border-t border-gray-50 dark:border-[#2c2c2c] bg-white dark:bg-[#1c1c1e] flex items-center justify-end relative" style="padding-bottom: max(env(safe-area-inset-bottom), 12px);">
              <div class="relative flex items-center p-1.5 rounded-md hover:bg-red-50 dark:hover:bg-red-900/20 text-red-400 hover:text-red-500 cursor-pointer transition-colors" title="Delete Project" @click.stop="showDeleteConfirm = true">
                  <span class="text-xs font-medium mr-2" v-if="!showDeleteConfirm">Delete Project</span>
                  <Trash2 class="w-[18px] h-[18px]" v-if="!showDeleteConfirm" />
              </div>
              
              <div v-if="showDeleteConfirm" class="absolute right-5 bg-white/90 dark:bg-[#1e1e1e]/90 flex items-center justify-end gap-2 rounded z-10 backdrop-blur-[2px]">
                  <span class="text-xs text-red-500 font-medium">Permanently delete project?</span>
                  <button @click.stop="emit('delete')" class="text-xs bg-red-500 text-white px-3 py-1.5 rounded font-medium hover:bg-red-600 transition-colors">Delete</button>
                  <button @click.stop="showDeleteConfirm = false" class="text-xs bg-gray-200 dark:bg-[#333] text-gray-700 dark:text-gray-300 px-3 py-1.5 rounded font-medium hover:bg-gray-300 dark:hover:bg-[#444] transition-colors">Cancel</button>
              </div>
          </div>
      </div>
  </div>
</template>
