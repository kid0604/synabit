<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { X, Target } from 'lucide-vue-next';
import type { Budget } from '../types';

const props = defineProps<{
    show: boolean;
    budget?: Budget | null;
    expenseCategories: string[];
    existingBudgetCategories: string[];
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'save', budget: Budget): void;
    (e: 'delete', id: string): void;
}>();

const name = ref<string>('');
const selectedCategories = ref<string[]>([]);
const amount = ref<string>('');

const isCategoryDisabled = (cat: string) => {
    if (selectedCategories.value.includes(cat)) return false;
    return props.existingBudgetCategories.includes(cat);
};

watch(() => props.show, (newVal) => {
    if (newVal) {
        if (props.budget) {
            name.value = props.budget.name || (props.budget as any).categoryId || '';
            selectedCategories.value = [...(props.budget.categories || ((props.budget as any).categoryId ? [(props.budget as any).categoryId] : []))];
            amount.value = props.budget.amount.toLocaleString('en-US');
        } else {
            name.value = '';
            selectedCategories.value = [];
            amount.value = '';
        }
    }
});

// Format number input with commas
const formatAmount = (val: string) => {
    const num = val.replace(/\D/g, '');
    if (!num) return '';
    return Number(num).toLocaleString('en-US');
};

const handleAmountInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    amount.value = formatAmount(target.value);
};

const canSave = computed(() => {
    const numericAmount = Number(amount.value.replace(/\D/g, ''));
    return name.value.trim() !== '' && selectedCategories.value.length > 0 && numericAmount > 0;
});

const save = () => {
    if (!canSave.value) return;
    
    emit('save', {
        id: props.budget?.id || `budget-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        name: name.value.trim(),
        categories: [...selectedCategories.value],
        amount: Number(amount.value.replace(/\D/g, ''))
    });
};

</script>

<template>
  <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 dark:bg-black/70 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-xl w-full max-w-sm overflow-hidden animate-in zoom-in-95 duration-200">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark bg-blue-50/50 dark:bg-blue-900/10">
        <h3 class="font-bold text-lg text-text dark:text-text-dark flex items-center gap-2">
            <Target class="w-5 h-5 text-blue-500" />
            {{ budget ? 'Edit Budget' : 'New Budget' }}
        </h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
            <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 space-y-4">
        <div v-if="expenseCategories.length === existingBudgetCategories.length && !budget" class="text-center p-4 text-sm text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-gray-800/50 rounded-xl">
            All expense categories have a budget!
        </div>
        <template v-else>
            <!-- Name -->
            <div class="space-y-1.5">
                <label class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Budget Name</label>
                <input 
                    type="text" 
                    v-model="name" 
                    class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500"
                    placeholder="e.g. Food & Dining"
                />
            </div>

            <!-- Categories -->
            <div class="space-y-1.5">
                <label class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Categories</label>
                <div class="max-h-40 overflow-y-auto hidden-scrollbar bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl p-3 flex flex-wrap gap-2">
                    <label 
                        v-for="cat in expenseCategories" 
                        :key="cat" 
                        class="flex items-center px-3 py-1.5 rounded-lg border transition-all cursor-pointer select-none text-sm"
                        :class="[
                            selectedCategories.includes(cat) 
                                ? 'bg-blue-50 border-blue-200 text-blue-700 dark:bg-blue-900/30 dark:border-blue-800 dark:text-blue-400 shadow-sm' 
                                : isCategoryDisabled(cat) && !selectedCategories.includes(cat)
                                    ? 'bg-gray-100 border-gray-200 text-gray-400 dark:bg-gray-800/50 dark:border-gray-700 dark:text-gray-500 cursor-not-allowed opacity-60'
                                    : 'bg-white border-gray-200 text-gray-700 hover:bg-gray-50 dark:bg-gray-900 dark:border-gray-700 dark:text-gray-300 dark:hover:bg-gray-800'
                        ]"
                    >
                        <input 
                            type="checkbox" 
                            :value="cat" 
                            v-model="selectedCategories"
                            :disabled="isCategoryDisabled(cat) && !selectedCategories.includes(cat)"
                            class="hidden"
                        />
                        <span>{{ cat }}</span>
                    </label>
                </div>
            </div>

            <!-- Amount -->
            <div class="space-y-1.5">
                <label class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Maximum budget</label>
                <div class="relative">
                    <input 
                        type="text" 
                        inputmode="numeric"
                        :value="amount"
                        @input="handleAmountInput"
                        class="w-full bg-transparent border border-border dark:border-border-dark rounded-xl px-4 py-3 text-2xl font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all pr-12"
                        placeholder="0"
                    />
                    <span class="absolute right-4 top-1/2 -translate-y-1/2 text-gray-400 font-medium pointer-events-none">$</span>
                </div>
            </div>
        </template>
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark bg-gray-50/50 dark:bg-gray-800/50 flex items-center justify-between gap-3">
        <button 
            v-if="budget" 
            @click="emit('delete', budget.id || (budget as any).categoryId)"
            class="px-4 py-2 text-sm font-medium text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 rounded-xl transition-colors"
        >
            Delete
        </button>
        <div v-else class="flex-1"></div>
        <div class="flex gap-3">
            <button @click="emit('close')" class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-xl transition-colors">
                Cancel
            </button>
            <button 
                @click="save"
                :disabled="!canSave"
                class="px-4 py-2 text-sm font-medium text-white bg-blue-500 hover:bg-blue-600 disabled:bg-blue-500/50 disabled:cursor-not-allowed rounded-xl transition-colors shadow-sm"
            >
                Save
            </button>
        </div>
      </div>

    </div>
  </div>
</template>
