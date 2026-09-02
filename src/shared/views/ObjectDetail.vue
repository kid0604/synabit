<script setup lang="ts">
/**
 * One node, whatever kind it is.
 *
 * Properties sit above the body rather than in a side rail, and that is the
 * one place this screen deliberately departs from Notes. For a note the body
 * is the content and `tags` is a remark about it; for a `book` or an `animal`
 * the frontmatter *is* the content and the body is an afterthought — Mèo Mun
 * is `species`, `colour` and `vaccinated_at` plus one sentence. Putting the
 * fields in a 280px rail would give the substance a column and the afterthought
 * the room.
 *
 * The proportion then takes care of itself: a book with six fields and no body
 * reads as a record, a note with two fields and three pages reads as a note.
 * Same layout, no mode to switch.
 */
import { ref, computed, nextTick } from 'vue';
import { Plus, X, Loader2, ChevronRight, ChevronDown, ExternalLink } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import TiptapEditor from '../../mini-apps/note/TiptapEditor.vue';
import { iconForNodeType } from './nodeTypeIcon';
import PropertyValue from './PropertyValue.vue';
import { humanizeKey } from '../fieldRegistry';
import type { FieldRow } from '../../mini-apps/things/composables/useThingsNode';

defineProps<{
  nodeType: string;
  readOnlyRows: { key: string; value: string }[];
  /**
   * The keys this app writes for itself, kept out of the list above.
   *
   * Passed in and shown behind a disclosure rather than dropped, because a
   * panel that silently omits part of a file is a panel you cannot trust to
   * tell you what is in the file.
   */
  appFields: FieldRow[];
  loading?: boolean;
  saving?: boolean;
  vaultPath?: string;
  /**
   * Where this kind is really edited, when it is not here.
   *
   * A whiteboard's content is a graph of nodes and edges, not a body of text.
   * Drawing an empty editor under it invites somebody to type into a file that
   * has no room for what they typed.
   */
  authoredIn?: string | null;
  /** Let the body run the width of the pane rather than a reading column. */
  fullWidth?: boolean;
  /**
   * Whether the kind can still be changed.
   *
   * True only while the node is a draft. Once the file exists its kind decides
   * which folder it lives in, so changing it would be a move rather than an
   * edit — and this pane does not move files.
   */
  typeEditable?: boolean;
  /**
   * The node's own id, which the editor needs to be able to link into it.
   *
   * Right-clicking a paragraph offers a link to that block, and the link has
   * to name the file it points at. Without this the whole feature is off —
   * silently, since the menu simply does not appear.
   */
  nodeId?: string;
  /** Passed through so zen mode scrolls the way it does in Notes. */
  zenMode?: boolean;
}>();

const title = defineModel<string>('title', { required: true });
const body = defineModel<string>('body', { required: true });
const fields = defineModel<FieldRow[]>('fields', { required: true });

const emit = defineEmits<{
  save: [];
  /** A link inside the body was followed. The owner decides what to open. */
  openNode: [id: string, type: string];
  addField: [];
  /**
   * Take this field off the node.
   *
   * The save is the listener's to make, not this one's: it used to fire here
   * on the same click, so a confirmation had already been answered by the
   * write before anyone was asked.
   */
  removeField: [index: number];
  pickType: [at: { x: number; y: number }];
  pickField: [at: { x: number; y: number }];
  /** Hand this node to the app that knows how to edit it. */
  openOwner: [];
}>();

/** Collapsed by default; the point of hiding them is that they are noise. */
const showAppFields = ref(false);

/**
 * Which field's name is being edited, if any.
 *
 * The name is shown as words and edited as the key. Those have to be two
 * states rather than one box, because the box is bound to the key itself:
 * showing `Due date` in something editable means the first keystroke renames
 * the field to `Due date`, and every query written against `due_date` stops
 * matching.
 */
const renaming = ref<number | null>(null);
const startRenaming = async (index: number) => {
  renaming.value = index;
  await nextTick();
  keyInput.value?.[0]?.focus();
};
const keyInput = ref<HTMLInputElement[] | null>(null);

/**
 * Enter in a field's name goes to its value, rather than nowhere.
 *
 * Naming a field and saying what it holds is one thought, and Enter used to
 * end it halfway — the name was kept, the cursor was dropped, and the value
 * needed a click to reach. Both halves now leave by the same key.
 */
/**
 * A new field arrives with the cursor already in its name.
 *
 * Adding one and then having to click the box that just appeared is the same
 * small tax as Enter going nowhere: the button says what you want, so the app
 * should be ready for the next thing you were always going to type.
 */
/**
 * Ask for a field rather than opening an empty one.
 *
 * The empty box is still reachable, one step further in, behind "new field".
 * That ordering is the point: the kind's existing keys are what you land on,
 * so adding `colour` costs a click and inventing `màu` costs a decision.
 */
const addField = (event: MouseEvent) => {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  emit('pickField', { x: box.left, y: box.bottom });
};

/** Put the cursor in the value of the row just added, whichever it is. */
const focusValue = async (index: number) => {
  await nextTick();
  valueBox.value?.[index]?.focus();
};

const valueBox = ref<{ focus: () => void }[] | null>(null);

/**
 * How many property rows are worth showing before they become a wall.
 *
 * A task carries ten and a kind somebody has been refining can carry fifty.
 * Past a point the list stops being information about this node and becomes
 * the shape of its kind, repeated on every node of that kind.
 */
const SHOWN = 8;

const collapsed = ref(false);
const expanded = ref(false);

/** Rows carry their own index, because editing and removing are by position. */
const rows = computed(() => fields.value.map((field, index) => ({ field, index })));

/**
 * What to show first: the fields that say something.
 *
 * An empty row is an offer — the kind's shape, laid out ready to fill in — and
 * an offer is worth less than an answer. So the answers come first and the
 * offers wait behind "show more".
 *
 * Except when there are no answers at all. A node just created has nothing but
 * offers, and hiding them would hand somebody a blank page where the whole
 * point was to have the shape in front of them.
 */
const visibleRows = computed(() => {
  if (expanded.value) return rows.value;
  const answered = rows.value.filter(r => r.field.value.trim());
  return (answered.length ? answered : rows.value).slice(0, SHOWN);
});

const hiddenCount = computed(() => rows.value.length - visibleRows.value.length);
const toValue = async (index: number) => {
  renaming.value = null;
  emit('save');
  await nextTick();
  valueBox.value?.[index]?.focus();
};

/** The picker opens under the chip, so it needs where the chip is. */
const chipAnchor = (event: MouseEvent) => {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  return { x: box.left, y: box.bottom };
};

const { t } = useI18n();

/**
 * The title box, so a node that was just created can be named without
 * reaching for the mouse.
 *
 * Creating a thing and naming it are one gesture. Splitting them is how the
 * vault fills with files called Untitled.
 */
const titleInput = ref<HTMLInputElement | null>(null);


defineExpose({ focusTitle: () => titleInput.value?.focus(), focusValue });
</script>

<template>
  <div class="h-full flex flex-col min-h-0 bg-white dark:bg-[#141416]">
    <div v-if="loading" class="flex-1 flex items-center justify-center text-gray-400">
      <Loader2 class="w-5 h-5 animate-spin" />
    </div>

    <div v-else class="flex-1 overflow-y-auto min-h-0">
      <!--
        The reading column, or the whole pane. `w-full` matters as much as the
        max-width: without it the div has nothing to fill once the cap is
        lifted. Notes carries the same pair for the same reason.
      -->
      <div
        class="mx-auto w-full px-8 py-7 transition-all duration-300"
        :class="fullWidth ? 'max-w-none' : 'max-w-3xl'"
      >

        <!--
          The type's own word. Not translated, for the reason the left rail is
          not: a type nobody wrote code for has no other name.

          While it is a draft the word is the control for changing it, which is
          what lets creating start with the name of the thing instead of a
          question about its kind — get the kind wrong and it is one click,
          right here, before anything is written.
        -->
        <div class="flex items-center gap-2 mb-3">
          <button
            v-if="typeEditable"
            type="button"
            @click="emit('pickType', chipAnchor($event))"
            class="flex items-center gap-2 -mx-1.5 px-1.5 py-0.5 rounded-md cursor-pointer
                   hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
          >
            <component :is="iconForNodeType(nodeType)" class="w-4 h-4 text-gray-400" />
            <span class="text-xs font-mono text-gray-500 dark:text-gray-400">{{ nodeType }}</span>
            <ChevronDown class="w-3 h-3 text-gray-400" />
          </button>
          <template v-else>
            <component :is="iconForNodeType(nodeType)" class="w-4 h-4 text-gray-400" />
            <span class="text-xs font-mono text-gray-400 dark:text-gray-500">{{ nodeType }}</span>
          </template>
        </div>

        <input
          ref="titleInput"
          v-model="title"
          type="text"
          :placeholder="t('things.untitled')"
          @blur="emit('save')"
          @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
          class="w-full bg-transparent border-0 outline-none text-3xl font-bold tracking-tight
                 text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 dark:placeholder-gray-600 mb-6"
        />

        <!-- ── Properties ─────────────────────────────────── -->
        <!--
          A heading that is also the switch.
          
          The properties are the substance of a `book` and a remark on a note,
          and the same screen draws both — so whether they are worth the top of
          the page is the reader's call, not this component's.
        -->
        <button
          v-if="rows.length || readOnlyRows.length"
          type="button"
          @click="collapsed = !collapsed"
          class="flex items-center gap-1.5 mb-2 -ml-1 px-1 py-0.5 rounded
                 text-[11px] uppercase tracking-wider cursor-pointer
                 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
        >
          <ChevronRight class="w-3 h-3 transition-transform" :class="collapsed ? '' : 'rotate-90'" />
          {{ t('things.properties') }}
          <span v-if="collapsed && rows.length" class="text-gray-300 dark:text-gray-600">
            {{ rows.length }}
          </span>
        </button>

        <div v-show="!collapsed" class="space-y-1.5 mb-7">
          <div
            v-for="row in readOnlyRows.filter(r => r.key !== 'type' && r.key !== 'title')"
            :key="row.key"
            class="flex items-start gap-3 text-sm"
          >
            <span class="w-[130px] flex-none pt-1 text-gray-400 dark:text-gray-500 font-mono text-xs truncate">
              {{ row.key }}
            </span>
            <span class="flex-1 min-w-0 pt-1 text-gray-500 dark:text-gray-400">{{ row.value }}</span>
          </div>

          <div
            v-for="{ field, index } in visibleRows"
            :key="index"
            class="flex items-start gap-3 group"
          >
            <!--
              The name reads as words and the raw key stays on hover, because
              the raw key is what a query is written against: somebody who sees
              "Due date" and types `Due date:tomorrow` gets nothing back.
            -->
            <input
              v-if="renaming === index || !field.key"
              ref="keyInput"
              v-model="field.key"
              spellcheck="false"
              :placeholder="t('things.field_name')"
              @blur="renaming = null; emit('save')"
              @keydown.enter.prevent="toValue(index)"
              class="w-[130px] flex-none px-2 py-1 rounded bg-gray-50 dark:bg-white/5
                     border border-gray-200 dark:border-gray-700
                     outline-none text-xs font-mono
                     text-gray-500 dark:text-gray-400 placeholder-gray-300 transition-colors"
            />
            <!--
              Words to read, the key underneath to edit. The raw key stays on
              hover, because it is what a query is written against: somebody
              who sees "Due date" and types `Due date:tomorrow` gets nothing.
            -->
            <button
              v-else
              type="button"
              :title="field.key"
              @click="startRenaming(index)"
              class="w-[130px] flex-none px-2 py-1 pt-1 text-left rounded truncate
                     text-xs text-gray-400 dark:text-gray-500 cursor-text
                     hover:bg-gray-50 dark:hover:bg-white/5 transition-colors"
            >
              {{ humanizeKey(field.key) }}
            </button>
            <PropertyValue
              ref="valueBox"
              v-model="field.value"
              :kind="field.kind"
              :placeholder="t('things.field_value')"
              @change="emit('save')"
            />
            <button
              type="button"
              @click="emit('removeField', index)"
              class="p-1 mt-1 rounded text-gray-300 hover:text-red-500 transition-colors cursor-pointer
                     opacity-0 group-hover:opacity-100 focus:opacity-100"
              :aria-label="t('things.remove_field')"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <!--
            The rest of them. Counted, so the button says how much is behind it
            rather than inviting a click to find out.
          -->
          <button
            v-if="hiddenCount > 0"
            type="button"
            @click="expanded = true"
            class="flex items-center gap-1.5 mt-1 px-2 py-1 text-xs text-gray-400
                   hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <ChevronRight class="w-3 h-3" />
            {{ t('things.show_more_fields', { count: hiddenCount }) }}
          </button>
          <button
            v-else-if="expanded && rows.length > SHOWN"
            type="button"
            @click="expanded = false"
            class="flex items-center gap-1.5 mt-1 px-2 py-1 text-xs text-gray-400
                   hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <ChevronRight class="w-3 h-3 -rotate-90" />
            {{ t('things.show_fewer_fields') }}
          </button>

          <button
            type="button"
            @click="addField($event)"
            class="flex items-center gap-1.5 mt-1 px-2 py-1 text-xs text-gray-400
                   hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <Plus class="w-3 h-3" /> {{ t('things.add_field') }}
          </button>

          <!--
            The app's own keys. Hidden because `pinned  false` on a hundred
            notes tells nobody anything, and reachable because a file's
            contents should never be a secret from the person who owns it.
          -->
          <div v-if="appFields.length" class="pt-1">
            <button
              type="button"
              @click="showAppFields = !showAppFields"
              class="flex items-center gap-1 px-2 py-1 text-xs text-gray-400
                     hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
            >
              <ChevronRight
                class="w-3 h-3 transition-transform"
                :class="showAppFields ? 'rotate-90' : ''"
              />
              {{ t('things.app_fields', { count: appFields.length }) }}
            </button>

            <div v-if="showAppFields" class="space-y-1.5 mt-1 pl-1">
              <div v-for="field in appFields" :key="field.key" class="flex items-start gap-3">
                <span
                  :title="field.key"
                  class="w-[130px] flex-none pt-1 text-xs text-gray-400 dark:text-gray-500 truncate"
                >
                  {{ humanizeKey(field.key) }}
                </span>
                <PropertyValue
                  v-model="field.value"
                  :kind="field.kind"
                  readonly
                />
              </div>
            </div>
          </div>
        </div>

        <!-- ── Body ───────────────────────────────────────── -->
        <!--
          Not every kind has one. A board's content is a drawing, and an empty
          editor beneath it is an invitation to type into a file with nowhere
          to put it.
        -->
        <div
          v-if="authoredIn"
          class="border-t border-gray-100 dark:border-[#232326] pt-6"
        >
          <button
            type="button"
            @click="emit('openOwner')"
            class="flex items-center gap-2 px-3 py-2 rounded-lg text-xs cursor-pointer
                   text-gray-600 dark:text-gray-300 border border-gray-200 dark:border-gray-700
                   hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
          >
            <ExternalLink class="w-3.5 h-3.5 text-gray-400" />
            {{ t('things.open_in', { app: authoredIn }) }}
          </button>
          <p class="mt-2 text-[11px] text-gray-400 leading-relaxed max-w-sm">
            {{ t('things.not_text_here') }}
          </p>
        </div>

        <!--
          Room under the last line, the same `pb-20` the Notes editor carries.
          
          Not decoration. A body whose last line sits on the pane's edge cannot
          be scrolled anywhere comfortable to read, and clicking below the text
          — how anybody puts the cursor at the end — has almost no target to
          hit. This had the container's 32px and nothing of its own.
        -->
        <div v-else class="border-t border-gray-100 dark:border-[#232326] pt-6 pb-20">
          <TiptapEditor
            v-model="body"
            :vaultPath="vaultPath || ''"
            :minHeightClass="'min-h-[160px]'"
            :placeholder="t('things.body_placeholder')"
            :currentNoteId="nodeId"
            :zenMode="zenMode"
            class="w-full"
            @blur="emit('save')"
            @open-internal-note="p => emit('openNode', p.id, p.type)"
          />
        </div>
      </div>
    </div>

    <div
      v-if="saving"
      class="flex-shrink-0 px-8 py-1.5 text-[11px] text-gray-400 border-t border-gray-100 dark:border-[#232326]"
    >
      {{ t('things.saving') }}
    </div>
  </div>
</template>
