<script setup lang="ts">
import { ref } from 'vue';
import { ChevronLeft, ChevronRight, Plus, Calendar as CalendarIcon, Upload, Download, MoreHorizontal, Rss, Moon, Check } from 'lucide-vue-next';
import { showLunar } from '../lunarDisplay';
import AppHeader from '../../../shared/components/AppHeader.vue';
import type { ViewMode } from '../types';

defineProps<{
    headerDisplayString: string;
    viewMode: ViewMode;
}>();

const emit = defineEmits<{
    (e: 'update:viewMode', v: ViewMode): void;
    (e: 'export-ics'): void;
    (e: 'import-ics'): void;
    (e: 'subscriptions'): void;
    (e: 'navigate-prev'): void;
    (e: 'navigate-next'): void;
    (e: 'go-today'): void;
    (e: 'add-event'): void;
}>();

const VIEWS: ViewMode[] = ['day', 'week', 'month', 'year', 'agenda'];
const ACTIVE = 'bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1)] text-accent dark:bg-surface-hover-dark dark:text-accent-dark';
const INACTIVE = 'text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white';

const menuOpen = ref(false);
/**
 * Where the menu opens, measured from its button.
 *
 * On a phone the header's actions sit in a row that scrolls sideways, and a
 * scrolling box clips whatever hangs out of it — an absolutely placed menu
 * opened inside it and was cut off at the row's bottom edge. A fixed one is
 * placed against the window instead, so it is measured here.
 */
const menuButton = ref<HTMLElement | null>(null);
const menuPos = ref({ top: 0, right: 0 });
const toggleMenu = () => {
    if (!menuOpen.value && menuButton.value) {
        const r = menuButton.value.getBoundingClientRect();
        menuPos.value = { top: r.bottom + 4, right: Math.max(8, window.innerWidth - r.right) };
    }
    menuOpen.value = !menuOpen.value;
};
const toggleLunar = () => { showLunar.value = !showLunar.value; };
const choose = (what: 'export-ics' | 'import-ics' | 'subscriptions') => {
    menuOpen.value = false;
    // `emit` is an overload set, one signature per event, and TypeScript does
    // not distribute a union argument across it — it tries the last signature
    // alone and rejects everything else. Each branch passes a literal, which
    // picks a signature; the menu still closes in one place.
    switch (what) {
        case 'export-ics': emit('export-ics'); break;
        case 'import-ics': emit('import-ics'); break;
        case 'subscriptions': emit('subscriptions'); break;
    }
};
</script>

<template>
    <div class="shrink-0">
        <AppHeader
            :title="headerDisplayString"
            :icon="CalendarIcon"
            :primaryLabel="$t('calendar.new_event')"
            :primaryIcon="Plus"
            @primary="emit('add-event')"
        >
            <template #actions>
                <!-- View Switcher; on a phone it gets a row of its own below -->
                <div class="hidden md:flex bg-gray-100 dark:bg-surface-dark p-1 rounded-xl border border-gray-200 dark:border-border-subtle-dark shrink-0 select-none" role="group" :aria-label="$t('calendar.view')">
                   <button v-for="v in VIEWS" :key="v"
                           @click="emit('update:viewMode', v)"
                           :aria-pressed="viewMode === v"
                           class="px-3 py-1.5 text-xs font-semibold rounded-lg transition-all"
                           :class="viewMode === v ? ACTIVE : INACTIVE">
                       {{ $t('calendar.view_' + v) }}
                   </button>
                </div>

                <!-- Nav Controls -->
                <button @click="emit('go-today')" class="btn-secondary shrink-0">
                    {{ $t('calendar.today') }}
                </button>
                <div class="flex items-center shrink-0">
                    <button @click="emit('navigate-prev')" class="btn-icon" :aria-label="$t('calendar.a11y_prev_period')" :title="$t('calendar.a11y_prev_period')"><ChevronLeft class="w-5 h-5" aria-hidden="true" /></button>
                    <button @click="emit('navigate-next')" class="btn-icon" :aria-label="$t('calendar.a11y_next_period')" :title="$t('calendar.a11y_next_period')"><ChevronRight class="w-5 h-5" aria-hidden="true" /></button>
                </div>

                <div class="relative">
                    <button ref="menuButton" @click="toggleMenu"
                            :aria-expanded="menuOpen" aria-haspopup="menu"
                            :aria-label="$t('calendar.exchange')" :title="$t('calendar.exchange')"
                            class="btn-icon">
                        <MoreHorizontal class="w-4 h-4" />
                    </button>
                    <template v-if="menuOpen">
                        <!-- Clicking anywhere else puts it away, including on
                             a touch screen where there is no blur to rely on. -->
                        <div class="fixed inset-0 z-40" @click="menuOpen = false"></div>
                        <div role="menu"
                             :style="{ top: `${menuPos.top}px`, right: `${menuPos.right}px` }"
                             class="fixed z-50 w-52 py-1 rounded-xl bg-white dark:bg-surface-dark border border-border dark:border-border-subtle-dark shadow-lg"
                             @keydown.esc="menuOpen = false">
                            <button role="menuitem" @click="choose('export-ics')"
                                    class="w-full flex items-center gap-2 px-3 py-2 text-left text-[12px] font-medium hover:bg-gray-100 dark:hover:bg-surface-hover-dark transition-colors">
                                <Download class="w-3.5 h-3.5 shrink-0" />{{ $t('calendar.export_ics') }}
                            </button>
                            <button role="menuitem" @click="choose('import-ics')"
                                    class="w-full flex items-center gap-2 px-3 py-2 text-left text-[12px] font-medium hover:bg-gray-100 dark:hover:bg-surface-hover-dark transition-colors">
                                <Upload class="w-3.5 h-3.5 shrink-0" />{{ $t('calendar.import_ics') }}
                            </button>
                            <div class="my-1 border-t border-border dark:border-border-dark"></div>
                            <button role="menuitem" @click="choose('subscriptions')"
                                    class="w-full flex items-center gap-2 px-3 py-2 text-left text-[12px] font-medium hover:bg-gray-100 dark:hover:bg-surface-hover-dark transition-colors">
                                <Rss class="w-3.5 h-3.5 shrink-0" />{{ $t('calendar.subscriptions') }}
                            </button>
                            <div class="my-1 border-t border-border dark:border-border-dark"></div>
                            <!-- A setting rather than an action, so the menu
                                 stays open and the tick shows the change. -->
                            <button role="menuitemcheckbox" :aria-checked="showLunar" @click="toggleLunar"
                                    class="w-full flex items-center gap-2 px-3 py-2 text-left text-[12px] font-medium hover:bg-gray-100 dark:hover:bg-surface-hover-dark transition-colors">
                                <Moon class="w-3.5 h-3.5 shrink-0" /><span class="flex-1">{{ $t('calendar.lunar_toggle') }}</span>
                                <Check v-if="showLunar" class="w-3.5 h-3.5 shrink-0 text-accent dark:text-accent-dark" aria-hidden="true" />
                            </button>
                        </div>
                    </template>
                </div>
            </template>
        </AppHeader>

        <div class="md:hidden flex bg-gray-100 dark:bg-surface-dark p-1 mx-3 mt-2 rounded-xl border border-gray-200 dark:border-border-subtle-dark select-none" role="group" :aria-label="$t('calendar.view')">
           <button v-for="v in VIEWS" :key="v"
                   @click="emit('update:viewMode', v)"
                   :aria-pressed="viewMode === v"
                   class="flex-1 px-3 py-1.5 text-xs font-semibold rounded-lg transition-all"
                   :class="viewMode === v ? ACTIVE : INACTIVE">
               {{ $t('calendar.view_' + v) }}
           </button>
        </div>
    </div>
</template>
