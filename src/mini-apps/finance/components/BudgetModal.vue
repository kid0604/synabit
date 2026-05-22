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
    (e: 'delete', categoryId: string): void;
}>();

const categoryId = ref<string>('');
const amount = ref<string>('');

const availableCategories = computed(() => {
    // If editing, show the current category too
    if (props.budget) {
        return props.expenseCategories.filter(c => c === props.budget!.categoryId || !props.existingBudgetCategories.includes(c));
    }
    // If adding, only show categories that don't have a budget yet
    return props.expenseCategories.filter(c => !props.existingBudgetCategories.includes(c));
});

watch(() => props.show, (newVal) => {
    if (newVal) {
        if (props.budget) {
            categoryId.value = props.budget.categoryId;
            amount.value = props.budget.amount.toLocaleString('en-US');
        } else {
            categoryId.value = availableCategories.value.length ? availableCategories.value[0] : '';
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
    return categoryId.value && numericAmount > 0;
});

const save = () => {
    if (!canSave.value) return;
    
    emit('save', {
        categoryId: categoryId.value,
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
        <div v-if="availableCategories.length === 0 && !budget" class="text-center p-4 text-sm text-gray-500 dark:text-gray-400 bg-gray-50 dark:bg-gray-800/50 rounded-xl">
            All expense categories have a budget!
        </div>
        <template v-else>
            <!-- Category -->
            <div class="space-y-1.5">
                <label class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Category</label>
                <select 
                    v-model="categoryId" 
                    :disabled="!!budget"
                    class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none disabled:opacity-50"
                >
                    <option v-for="cat in availableCategories" :key="cat" :value="cat">{{ cat }}</option>
                </select>
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
            @click="emit('delete', budget.categoryId)"
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
                v-if="availableCategories.length > 0 || budget"
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
