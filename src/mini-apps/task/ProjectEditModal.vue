<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { X, Calendar, Trash2, Tag, Activity, PlusCircle, DollarSign } from 'lucide-vue-next';
import TiptapEditor from '../note/TiptapEditor.vue';
import AppDialog from '../../shared/components/AppDialog.vue';

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
const titleInput = ref<HTMLInputElement | null>(null);

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

/**
 * A write names the keys it is changing and leaves the rest of the file alone,
 * so a field the user cleared has to be named too — as `null` — or it simply
 * stays at its old value and the change appears not to have taken.
 *
 * Everything this form can empty goes through here: a custom property whose
 * row was removed, and the WIP limit and budget, which are omitted rather than
 * blanked when their input is empty.
 */
const clearedKeys = (kept: Record<string, string>): Record<string, null> => {
    const cleared: Record<string, null> = {};
    for (const key of Object.keys(props.project?.custom_fields || {})) {
        // Only the keys this form governs. It never shows `created_at`, `type`
        // or the node's `node_id` — they are filtered out of the rows above —
        // so their absence from the payload means the form has nothing to say
        // about them, not that they should go. Nulling those would reset every
        // project's creation date and, in the case of `node_id`, hand the file
        // a fresh identity and split it into two documents on the next sync.
        const governed = !standardKeys.includes(key.toLowerCase())
            || key === 'wip_limit'
            || key === 'budget';
        if (governed && !(key in kept)) cleared[key] = null;
    }
    return cleared;
};

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
        custom_fields: { ...clearedKeys(custom_fields), ...custom_fields }
    });
};

const _close = () => {
    emit('close');
};

const handleBackgroundClick = () => {
    save();
};
</script>

<template>
  <!-- Escape and the scrim save, as clicking outside always did here. -->
  <AppDialog
      :show="true"
      :ariaLabel="$t('task.edit_project')"
      :initialFocus="() => titleInput"
      unstyled
      @close="handleBackgroundClick"
  >
      <div class="w-full max-h-[90vh] bg-white dark:bg-surface-dark rounded-2xl shadow-[0_20px_40px_rgba(0,0,0,0.1)] dark:shadow-[0_20px_40px_rgba(0,0,0,0.4)] border border-gray-100 dark:border-border-dark overflow-hidden flex flex-col">
          
          <div class="flex justify-between items-center px-5 pt-4 pb-4 md:hidden shrink-0 border-b border-gray-100 dark:border-border-dark">
              <h3 class="font-semibold text-lg text-text dark:text-text-dark">{{ $t('task.edit_project') }}</h3>
              <button @click="handleBackgroundClick" class="p-2 -mr-2 text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 rounded-full bg-gray-100 dark:bg-[#2c2c2c]" :aria-label="$t('task.a11y_save_close')" :title="$t('task.a11y_save_close')">
                  <X class="w-4 h-4" />
              </button>
          </div>

          <div class="p-5 flex flex-col pt-5 md:pt-6 flex-1 overflow-y-auto">
              <div class="flex items-start gap-4 mb-4">
                   <input 
                       ref="titleInput"
                       v-model="editingProject.title" 
                       class="flex-1 bg-transparent border-none outline-none text-[1.1rem] font-medium text-text dark:text-text-dark placeholder-gray-300 focus:ring-0 p-0 leading-snug"
                       :placeholder="$t('task.project_title_placeholder')"
                   />
              </div>
              
              <!-- Standard Properties -->
              <div class="mb-1.5 space-y-1.5">
                  <!-- Status -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Activity class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.status') }}</div>
                      <select v-model="editingProject.status" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-text dark:text-text-dark font-medium appearance-none cursor-pointer">
                          <option value="active">{{ $t('task.project_status_active') }}</option>
                          <option value="on_hold">{{ $t('task.project_status_on_hold') }}</option>
                          <option value="completed">{{ $t('task.project_status_completed') }}</option>
                      </select>
                      <div class="w-[22px]"></div> <!-- Spacer to align with custom properties X button -->
                  </div>
                  
                  <!-- Dates -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Calendar class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.dates') }}</div>
                      <div class="flex-1 flex items-center gap-1 bg-gray-50 dark:bg-[#2c2c2c] rounded px-1.5 border border-transparent focus-within:border-gray-200 dark:focus-within:border-gray-700 transition-colors">
                          <input type="date" v-model="editingProject.start_date" class="w-full text-xs bg-transparent border-none outline-none text-text dark:text-text-dark py-1.5 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" :aria-label="$t('task.a11y_project_start')" />
                          <span class="text-gray-500 dark:text-gray-400 text-xs px-1">→</span>
                          <input type="date" v-model="editingProject.due_date" class="w-full text-xs bg-transparent border-none outline-none text-text dark:text-text-dark py-1.5 [color-scheme:light] dark:[color-scheme:dark] cursor-pointer" :aria-label="$t('task.a11y_project_due')" />
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- WIP Limit -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Activity class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.wip_limit') }}</div>
                      <input type="number" min="1" v-model="wipLimitInput" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-text dark:text-text-dark font-medium" :placeholder="$t('task.example_placeholder', { value: 5 })" />
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- Budget -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><DollarSign class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.budget') }}</div>
                      <div class="flex-1 flex items-center gap-1">
                          <input type="text" :value="budgetInput" @input="handleBudgetInput" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-text dark:text-text-dark font-medium" :placeholder="$t('task.example_placeholder', { value: '10,000,000' })" />
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
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><DollarSign class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.spent') }}</div>
                      <div class="flex-1 text-xs bg-gray-100 dark:bg-[#222] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium cursor-not-allowed flex items-center justify-between">
                          <span>{{ spentDisplay }}</span>
                          <span class="text-xs font-bold text-gray-500 dark:text-gray-400">{{ rawCurrency }}</span>
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
                  
                  <!-- Tags -->
                  <div class="flex items-center gap-2">
                      <div class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent rounded p-1.5 text-gray-500 dark:text-gray-400 font-medium flex items-center"><Tag class="w-3 h-3 mr-2 opacity-70"/> {{ $t('task.tags') }}</div>
                      <div class="flex-1 flex flex-wrap items-center gap-1.5 bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus-within:border-gray-200 dark:focus-within:border-gray-700 rounded p-1 transition-colors min-h-[32px]">
                          <span v-for="(tag, index) in editingProject.tags" :key="index" class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm bg-gray-200 dark:bg-[#3d3d3d] text-gray-700 dark:text-gray-300 text-xs font-medium">
                              #{{ tag }}
                              <button @click.prevent="removeTag(index)" class="hover:text-red-500 focus:outline-none" :aria-label="$t('task.a11y_remove_tag')"><X class="w-3 h-3"/></button>
                          </span>
                          <input v-model="tagInput" @keydown.enter.prevent="addTag" @blur="addTag" :placeholder="$t('task.add_tag_placeholder')" class="flex-1 min-w-[80px] text-xs bg-transparent border-none outline-none text-text dark:text-text-dark placeholder-gray-500 dark:placeholder-gray-400 py-0.5 px-1" />
                      </div>
                      <div class="w-[22px]"></div>
                  </div>
              </div>
              
              <!-- Custom Properties -->
              <div v-if="customProperties.length > 0" class="mb-1.5 space-y-1.5">
                  <div v-for="(prop, index) in customProperties" :key="index" class="flex items-center gap-2 group relative">
                      <input v-model="prop.key" :placeholder="$t('task.field_name')" class="w-[120px] text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-gray-500 dark:text-gray-400 font-medium placeholder-gray-300" />
                      <input v-model="prop.value" :placeholder="$t('task.field_value')" class="flex-1 text-xs bg-gray-50 dark:bg-[#2c2c2c] border border-transparent focus:border-gray-200 dark:focus:border-gray-700 rounded p-1.5 outline-none text-text dark:text-text-dark placeholder-gray-300" />
                      
                      <div v-if="confirmDeleteIndex === index" class="absolute right-0 top-0 bottom-0 bg-white/90 dark:bg-surface-dark/90 flex items-center justify-end px-2 gap-2 rounded z-10 w-full backdrop-blur-[2px]">
                          <span class="text-xs text-red-500 font-medium">{{ $t('task.delete_field_confirm') }}</span>
                          <button @click.stop="removeCustomProperty(index)" class="text-xs bg-red-500 text-white px-2 py-1 rounded font-medium hover:bg-red-600 transition-colors">{{ $t('task.delete_confirm') }}</button>
                          <button @click.stop="confirmDeleteIndex = null" class="text-xs bg-gray-200 dark:bg-[#333] text-gray-700 dark:text-gray-300 px-2 py-1 rounded font-medium hover:bg-gray-300 dark:hover:bg-[#444] transition-colors">{{ $t('task.delete_cancel') }}</button>
                      </div>
                      <button v-else @click="confirmDeleteIndex = index" class="p-1 text-gray-500 dark:text-gray-400 hover:text-red-500 rounded transition-colors" :aria-label="$t('task.remove_field')"><X class="w-3.5 h-3.5" /></button>
                  </div>
              </div>
              <div class="mb-4">
                  <button @click="addCustomProperty" class="text-xs font-medium text-gray-500 dark:text-gray-400 hover:text-accent flex items-center transition-colors">
                      <PlusCircle class="w-3 h-3 mr-1" /> {{ $t('task.add_field') }}
                  </button>
              </div>
              
              <div class="mb-4 flex-1 flex flex-col min-h-[100px] max-h-[300px] overflow-y-auto overflow-x-hidden custom-scrollbar border-t border-gray-100 dark:border-border-dark pt-4">
                  <TiptapEditor 
                       v-model="editingProject.content" 
                       :vaultPath="props.vaultPath || ''"
                       :minHeightClass="'min-h-[100px]'"
                       class="w-full flex-1"
                       :placeholder="$t('task.project_description_placeholder')"
                  />
              </div>
          </div>
          
          <div v-if="!project.isNew" class="px-5 pt-3 border-t border-gray-50 dark:border-border-dark bg-white dark:bg-[#1c1c1e] flex items-center justify-end relative" style="padding-bottom: max(env(safe-area-inset-bottom), 12px);">
              <!-- No question first: the delete waits under an Undo toast instead. -->
              <button type="button" class="relative flex items-center p-1.5 rounded-md hover:bg-red-50 dark:hover:bg-red-900/20 text-red-500 cursor-pointer transition-colors" @click.stop="emit('delete')">
                  <span class="text-xs font-medium mr-2">{{ $t('task.delete_project') }}</span>
                  <Trash2 class="w-[18px] h-[18px]" aria-hidden="true" />
              </button>
          </div>
      </div>
  </AppDialog>
</template>