<script setup lang="ts">
/**
 * Kệ thấu kính — the saved questions, and the way to save one.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, step 2.
 *
 * The shelf is deliberately plain: a row of the person's own questions, in
 * their own words, each one press away. What matters is what is *not* here —
 * no categories, no built-in section the person cannot edit, no lens with
 * privileges. A lens that ships with the vault and a lens somebody wrote this
 * morning are the same kind of thing and sit in the same row, because the
 * moment one of them is special the design is back to panels.
 *
 * Saving happens where the answer is, not in a settings screen: the moment a
 * person wants to keep a question is the moment they have just seen it answer
 * correctly.
 */
import { computed, ref, watch } from 'vue';
import { BookMarked, Plus, Trash2 } from 'lucide-vue-next';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';
import { lensPath, nameFor, normalise, propertiesOf, readShelf, type Lens } from '../../../shared/lenses';

const props = defineProps<{
    vaultPath: string;
    /** The question on the bar right now, which is what Save would keep. */
    query: string;
    /** And how it is being seen, which is kept with it. */
    render: string;
    /** Which lens the bar is showing, if it came from one. */
    active: string | null;
}>();

const emit = defineEmits<{
    (e: 'run', lens: Lens): void;
    (e: 'changed'): void;
}>();

const ns = useNodeService();
const shelf = ref<Lens[]>([]);
const naming = ref(false);
const name = ref('');

const savable = computed(() => props.query.trim().length > 0);
/** Already on the shelf under this exact question — saving again is noise. */
const kept = computed(() =>
    shelf.value.some(lens => lens.query.trim() === props.query.trim()),
);

const load = async () => {
    try {
        shelf.value = await readShelf(props.vaultPath);
    } catch (e) {
        logger.error('Could not read the lens shelf', e);
        shelf.value = [];
    }
};

watch(() => props.vaultPath, load, { immediate: true });

const startNaming = () => {
    naming.value = true;
    name.value = nameFor(props.query);
};

const save = async () => {
    const title = name.value.trim() || nameFor(props.query);
    try {
        await ns.writeNode({
            relPath: lensPath(),
            nodeType: 'lens' as never,
            title,
            properties: propertiesOf({
                title,
                query: props.query,
                render: normalise(props.render),
                icon: '',
            }),
            content: '',
            eventType: 'created',
        });
        naming.value = false;
        name.value = '';
        await load();
        emit('changed');
    } catch (e) {
        logger.error('Could not save the lens', e);
    }
};

const forget = async (lens: Lens) => {
    try {
        await ns.trashNode({ relPath: lens.id });
        shelf.value = shelf.value.filter(l => l.id !== lens.id);
        emit('changed');
    } catch (e) {
        logger.error('Could not put the lens away', e);
    }
};
</script>

<template>
    <div data-lens-shelf class="flex flex-wrap items-center gap-1.5">
        <span class="mr-1 text-[10px] font-bold uppercase tracking-wider text-gray-400">
            {{ $t('nexus.lens_shelf') }}
        </span>

        <!-- Two real buttons side by side rather than one inside the other:
             a button nested in a button is invalid markup and the inner one is
             unreachable by keyboard, which would make putting a lens away a
             mouse-only gesture. -->
        <span
            v-for="lens in shelf"
            :key="lens.id"
            class="group inline-flex h-[30px] items-stretch overflow-hidden rounded-full border transition-colors"
            :class="
                lens.id === active
                    ? 'border-indigo-600 bg-indigo-600 text-white'
                    : 'border-gray-200 bg-white text-gray-700 dark:border-[#3a3a3c] dark:bg-[#242426] dark:text-gray-200'
            "
        >
            <button
                type="button"
                data-lens
                class="inline-flex items-center gap-1.5 pl-3 pr-2 text-xs font-medium"
                :title="lens.query"
                @click="emit('run', lens)"
            >
                <BookMarked class="h-3 w-3 flex-shrink-0 opacity-60" />
                {{ lens.title }}
            </button>
            <button
                type="button"
                data-forget
                :aria-label="$t('nexus.lens_forget')"
                class="inline-flex items-center pr-2.5 opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-60 hover:!opacity-100"
                @click="forget(lens)"
            >
                <Trash2 class="h-3 w-3" />
            </button>
        </span>

        <p v-if="!shelf.length" data-lens-empty class="text-[11px] text-gray-400">
            {{ $t('nexus.lens_none') }}
        </p>

        <!-- Saving sits beside the answer, not in a settings screen. -->
        <template v-if="naming">
            <input
                v-model="name"
                data-lens-name
                type="text"
                :aria-label="$t('nexus.lens_name')"
                :placeholder="$t('nexus.lens_name')"
                class="h-[30px] w-52 rounded-full border border-gray-300 bg-white px-3 text-xs text-gray-900 outline-none focus:border-indigo-500 dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                @keyup.enter="save"
                @keyup.esc="naming = false"
            />
            <button
                type="button"
                data-lens-keep
                class="h-[30px] rounded-full bg-indigo-600 px-3 text-xs font-semibold text-white"
                @click="save"
            >
                {{ $t('nexus.lens_keep') }}
            </button>
        </template>

        <button
            v-else-if="savable && !kept"
            type="button"
            data-lens-save
            class="inline-flex h-[30px] items-center gap-1 rounded-full border border-dashed border-gray-300 px-3 text-xs text-gray-600 transition-colors hover:border-indigo-400 hover:text-indigo-600 dark:border-[#3a3a3c] dark:text-gray-300"
            @click="startNaming"
        >
            <Plus class="h-3 w-3" /> {{ $t('nexus.lens_save') }}
        </button>
    </div>
</template>
