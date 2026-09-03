<script setup lang="ts">
/**
 * What a kind turns out to be, according to the vault.
 *
 * It fills the middle of the screen while a kind is being browsed and no node
 * is open — a space that previously said "pick a type" and did nothing else,
 * and which is already where somebody stands when they are thinking about the
 * kind rather than about one of its members. No new screen, no settings page
 * to remember to visit.
 *
 * Nothing here is a schema in the sense of a rule. Every row is a count of
 * files, and the counts are the whole argument: `colour 2/4` sitting above
 * `màu 1/4` explains the drift without a warning, a heuristic, or a word of
 * copy. Fields the app writes for itself, and keys too rare to be the kind's
 * shape, are separated out rather than hidden — the vault gets to disagree
 * with any idea of what a kind should be, and it says so here.
 */
import { ref, computed, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { ArrowRight, ArrowLeft, Plus, MoreHorizontal } from 'lucide-vue-next';
import { iconForNodeType } from './nodeTypeIcon';
import IconPicker from './IconPicker.vue';
import { humanizeKey, isAppOwned, GOVERNED } from '../fieldRegistry';
import type { FieldKind } from '../fieldValue';
import FieldKindPicker from './FieldKindPicker.vue';
import ShapeRowMenu from './ShapeRowMenu.vue';

const props = defineProps<{
  /** The icon name somebody chose for this kind, if they have. */
  chosenIcon?: string | null;
  nodeType: string;
  /** How many nodes of this kind the vault holds. */
  count: number;
  /** Every key on the kind, with how many nodes carry it. */
  fields: { key: string; count: number }[];
  /** The share of the kind a key needs before it counts as the kind's shape. */
  usual: string[];
  /** True once a schema file exists, so the shape is declared rather than guessed. */
  declared?: boolean;
  /**
   * The declared kind of each shape field.
   *
   * Only ever used to draw an empty box. A field with a value takes its kind
   * from the value — see `kindOf` — because a declaration that disagrees with
   * the file is wrong about the file, and changing this converts nothing.
   */
  kinds?: Record<string, FieldKind>;
  /**
   * What the vault turns out to hold for each key, where it holds anything.
   *
   * Absent for a key that is empty everywhere: that is the absence of evidence
   * rather than evidence of text, and warning about a disagreement with
   * nothing is how a warning stops being read.
   */
  observedKinds?: Record<string, FieldKind>;
}>();

const emit = defineEmits<{
  /** `null` puts the kind back to whatever the app draws by default. */
  pickIcon: [icon: string | null];
  /** Up to the list of every kind, which is this page's index. */
  back: [];
  /** Show the things themselves, rather than the structure of them. */
  browse: [];
  /** Take this kind out of the vault. The dialog decides what that means. */
  removeKind: [];
  /**
   * Say a different word on every node of this kind.
   *
   * Named apart from `rename`, which is a field's. They are the same operation
   * on two levels and the names have to stay apart, or a click on one fires
   * the other with a signature it does not have.
   */
  renameKind: [];
  /** Declare a field nothing carries yet. */
  addField: [key: string, kind: FieldKind];
  /** Change how an empty one of these is drawn. */
  setKind: [key: string, kind: FieldKind];
  /** Move a field up or down in the order a new node lays them out. */
  move: [key: string, by: number];
  /** Take a field out of the shape. Removes nothing from any file. */
  drop: [key: string];
  /** Put a field into the shape. */
  adopt: [key: string];
  /** Merge one key into another across every node of this kind. */
  rename: [from: string, to: string];
  /** End a key on every node of this kind. The only red thing on the page. */
  erase: [key: string];
}>();

const { t } = useI18n();

const iconPickerAt = ref<{ x: number; y: number } | null>(null);
const openIconPicker = (event: MouseEvent) => {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  iconPickerAt.value = { x: box.left, y: box.bottom };
};

const share = (n: number) => (props.count ? Math.round((n / props.count) * 100) : 0);

const own = computed(() =>
  props.fields.filter(f => !isAppOwned(props.nodeType, f.key) && !GOVERNED.has(f.key)),
);

/**
 * The kind's shape: what a new one of these arrives ready to hold.
 *
 * Built from the shape itself rather than by filtering what the vault has
 * seen, which is two fixes in one. A field declared and never used has no
 * observed row to filter down to, so it used to be declared and then
 * invisible — you could add `isbn` to `book` and watch nothing happen. And the
 * order is the shape's own, so moving a field up moves it on screen; filtering
 * the observed list gave back the vault's order every time, which made the
 * arrows look broken.
 */
const seenCount = (key: string) =>
  props.fields.find(f => f.key === key)?.count ?? 0;

const shape = computed(() =>
  props.usual.map(key => ({ key, count: seenCount(key) })),
);

/** The declared kind of a shape field, for the picker beside it. */
const kindOfShaped = (key: string): FieldKind =>
  props.kinds?.[key] ?? 'text';

/**
 * A declaration the files disagree with.
 *
 * Nothing is broken by it — a value is drawn by what it is, and saving
 * converts nothing — so this is a remark, not an error. What it costs is the
 * empty box: declaring `due_date` text on a hundred dated tasks means the next
 * one gets a text box for a date.
 */
const disagreement = (key: string): FieldKind | null => {
  const seen = props.observedKinds?.[key];
  return seen && seen !== kindOfShaped(key) ? seen : null;
};

/**
 * Real, on the files, and not part of the shape.
 *
 * Shown rather than dropped, because this is where a second word for one idea
 * becomes visible — and because a screen that quietly omits part of a file is
 * a screen you cannot use to find out what is in the file.
 */
const rest = computed(() => own.value.filter(f => !props.usual.includes(f.key)));

const machinery = computed(() =>
  props.fields.filter(f => isAppOwned(props.nodeType, f.key)),
);

/**
 * Declaring a field before anything has one.
 *
 * The `+` beside an observed key adopts something the vault already has. This
 * is the other half, and it is the half designing needs: deciding a `book` has
 * an `isbn` is a decision you make once, before any book has one, and there
 * was no way to say it.
 */
/** Which row has its kind picker open. One at a time, and none by default. */
const editingKind = ref<string | null>(null);

/** Where a row's menu should be drawn, from the button that asked for it. */
const anchor = (event: MouseEvent) => {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  return { x: box.right, y: box.bottom };
};

/** The row whose menu is open, and where to draw it. */
const menuFor = ref<string | null>(null);
const menuAt = ref<{ x: number; y: number } | null>(null);

const openMenu = (key: string, at: { x: number; y: number }) => {
  menuFor.value = key;
  menuAt.value = at;
};

const closeMenu = () => {
  menuFor.value = null;
  menuAt.value = null;
};

/** Where the row sits, so the menu can hide a move that would do nothing. */
const indexOf = (key: string) => props.usual.indexOf(key);

const adding = ref(false);
const draftKey = ref('');
const draftKind = ref<FieldKind>('text');
const draftBox = ref<HTMLInputElement | null>(null);

const startAdding = async () => {
  adding.value = true;
  await nextTick();
  draftBox.value?.focus();
};

const submitField = () => {
  const key = draftKey.value.trim();
  draftKey.value = '';
  adding.value = false;
  if (key) emit('addField', key, draftKind.value);
  draftKind.value = 'text';
};
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0 overflow-y-auto">
    <!--
      The same header the manager has, because this is that page's detail view
      and arriving here from it left nowhere to go: the pane simply became a
      kind, with no title bar and no way up.
    -->
    <header
      class="flex items-center gap-3 px-6 h-11 shrink-0 sticky top-0 z-10
             bg-white dark:bg-[#141416] border-b border-gray-100 dark:border-[#232326]"
    >
      <button
        type="button"
        @click="emit('back')"
        class="p-1.5 -ml-1.5 rounded-md text-gray-500 cursor-pointer
               hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
        :aria-label="t('things.back_to_kinds')"
        :title="t('things.back_to_kinds')"
      >
        <ArrowLeft class="w-4.5 h-4.5" />
      </button>
      <!--
        The icon is the button that changes it. No new control and no extra
        click: the thing on screen is the thing being edited, which is how the
        count above opens the table.
      -->
      <button
        type="button"
        @click="openIconPicker"
        :title="t('things.icon_change')"
        class="p-1 -m-1 rounded-md flex-none cursor-pointer transition-colors
               text-gray-400 hover:text-gray-600 dark:hover:text-gray-300
               hover:bg-gray-100 dark:hover:bg-white/10"
      >
        <component :is="iconForNodeType(nodeType)" class="w-4 h-4" />
      </button>
      <h1 class="text-base font-semibold text-text dark:text-text-dark">
        {{ nodeType }}
      </h1>
      <!--
        The count is the way to the things.
        
        This page describes a kind, and the obvious next question standing on
        it is "so show me them" — which was answerable only by finding a small
        layout icon back in the rail. The number somebody is already looking at
        is the thing to click.
      -->
      <button
        v-if="count"
        type="button"
        @click="emit('browse')"
        :title="t('things.browse_hint', { type: nodeType })"
        class="flex items-center gap-1 text-[11px] font-medium px-2 py-0.5 rounded-full
               cursor-pointer transition-colors tabular-nums
               bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400
               hover:bg-blue-50 dark:hover:bg-blue-900/30 hover:text-blue-600 dark:hover:text-blue-300"
      >
        {{ count }}
        <ArrowRight class="w-3 h-3" />
      </button>
      <span
        v-else
        class="text-[11px] font-medium px-2 py-0.5 rounded-full
               bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400 tabular-nums"
      >
        0
      </span>

      <div class="flex-1" />

      <!--
        Renaming a kind lived only inside the dialog for removing one, under
        the option to move its nodes elsewhere — the same act behind a bin.
        Nobody correcting a typed kind name looks under delete.
      -->
      <button
        type="button"
        @click="emit('renameKind')"
        class="px-2.5 py-1 rounded-md text-xs cursor-pointer transition-colors
               text-gray-500 dark:text-gray-400
               hover:bg-gray-100 dark:hover:bg-white/10"
      >
        {{ t('things.rename_kind_short') }}
      </button>

      <!--
        Two verbs, the same two the manager's row menu offers. A third that
        discarded only the declaration was there and has gone: on a kind with
        no files it did exactly what Remove does, and on one with files it was
        a distinction almost nobody wanted to draw.
      -->
      <button
        type="button"
        @click="emit('removeKind')"
        class="px-2.5 py-1 rounded-md text-xs cursor-pointer transition-colors
               text-gray-500 dark:text-gray-400
               hover:bg-red-50 dark:hover:bg-red-900/25
               hover:text-red-600 dark:hover:text-red-400"
      >
        {{ t('things.remove_kind_short') }}
      </button>
    </header>

    <div class="max-w-2xl mx-auto w-full px-8 py-8">
      <!--
        No width cap of its own: the column is already the measure.
        
        This carried `max-w-md` inside a `max-w-2xl` column, which is not a
        reading width, it is a second one — narrower than everything under it
        and just short enough to drop the last word onto a line by itself.
        `text-wrap: pretty` would tidy such a wrap, but it is below Baseline
        Newly available and unsupported outside Chromium and Safari 26, and it
        is not needed: with the column's own width the sentence fits a line.
      -->
      <p class="text-xs text-gray-400 dark:text-gray-500 mb-8 leading-relaxed">
        {{ declared ? t('things.shape_declared') : t('things.type_overview_note') }}
      </p>

      <!-- The shape: what a new one of these arrives holding. -->
      <section class="mb-7">
        <h3 class="text-[11px] font-medium uppercase tracking-wider text-gray-400 mb-2">
          {{ t('things.usual_fields') }}
        </h3>
        <!--
          A row of columns, not a row of everything.
          
          This grew one control at a time — a count, a kind, a warning, three
          verbs — until it held eleven things, `126 / 127` wrapped onto three
          lines, and no two rows lined up because the badge only appears on
          some. Each addition was reasonable and the total was not.
          
          So: fixed columns, so rows align whatever is in them. The verbs move
          into the row menu this app already uses for exactly this. And the
          kind is a word until you click it, because five pills on every row is
          two hundred pixels of choice nobody is making right now.
        -->
        <div
          v-for="field in shape"
          :key="field.key"
          class="group border-t border-gray-100 dark:border-[#232326]"
        >
          <div class="grid grid-cols-[minmax(0,1fr)_88px_92px_28px] items-center gap-3 py-2">
            <div class="min-w-0">
              <div class="text-sm text-[#1c1c1e] dark:text-[#f4f4f5] truncate">
                {{ humanizeKey(field.key) }}
              </div>
              <div class="font-mono text-[11px] text-gray-400 truncate">{{ field.key }}</div>
            </div>

            <span class="font-mono text-xs text-gray-400 tabular-nums text-right whitespace-nowrap">
              {{ field.count }} / {{ count }}
            </span>

            <!--
              The kind, and the files' opinion of it in the same place. Amber
              when they disagree: nothing is broken by that, but the next empty
              box will be drawn wrong, and this is where somebody can see it.
            -->
            <button
              type="button"
              @click="editingKind = editingKind === field.key ? null : field.key"
              :title="disagreement(field.key)
                ? t('things.kind_disagrees', {
                    declared: t(`things.kind_${kindOfShaped(field.key)}`),
                    seen: t(`things.kind_${disagreement(field.key)}`),
                  })
                : t('things.kind_change')"
              class="px-2 py-1 rounded-md text-[11px] whitespace-nowrap cursor-pointer
                     transition-colors justify-self-start"
              :class="disagreement(field.key)
                ? 'bg-amber-50 dark:bg-amber-900/25 text-amber-700 dark:text-amber-400'
                : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/10'"
            >
              {{ t(`things.kind_${kindOfShaped(field.key)}`) }}
              <span v-if="disagreement(field.key)"> ≠ {{ t(`things.kind_${disagreement(field.key)}`) }}</span>
            </button>

            <button
              type="button"
              @click="openMenu(field.key, anchor($event))"
              :title="t('things.row_actions')"
              class="p-1 rounded-md text-gray-400 cursor-pointer justify-self-end
                     hover:bg-gray-100 dark:hover:bg-white/10 transition-colors"
            >
              <MoreHorizontal class="w-4 h-4" />
            </button>
          </div>

          <!-- Opened by the word above, so only the row being changed grows. -->
          <div v-if="editingKind === field.key" class="pb-2 flex justify-end">
            <FieldKindPicker
              :model-value="kindOfShaped(field.key)"
              @update:model-value="k => { emit('setKind', field.key, k); editingKind = null; }"
            />
          </div>
        </div>

        <p v-if="!shape.length" class="py-2 text-xs text-gray-400">
          {{ t('things.no_shape_yet') }}
        </p>

        <!--
          Declaring a field before anything has one.
          
          The button beside an observed key adopts what the vault already
          holds; this is the other half, and the half designing needs. Deciding
          a book has an `isbn` is a decision made once, before any book has
          one — and until now there was no way to say it.
        -->
        <div v-if="adding" class="flex items-center gap-2 mt-2">
          <input
            ref="draftBox"
            v-model="draftKey"
            spellcheck="false"
            :placeholder="t('things.field_name')"
            @keydown.enter.prevent="submitField"
            @keydown.esc="adding = false; draftKey = ''"
            class="flex-1 min-w-0 px-2.5 py-1.5 rounded-lg font-mono text-xs outline-none
                   bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700
                   text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300"
          />
          <FieldKindPicker v-model="draftKind" />
          <button
            type="button"
            @click="submitField"
            class="px-2.5 py-1.5 rounded-lg text-xs font-medium text-white bg-blue-600
                   hover:bg-blue-700 cursor-pointer flex-none"
          >
            {{ t('things.add') }}
          </button>
        </div>
        <button
          v-else
          type="button"
          @click="startAdding"
          class="flex items-center gap-1.5 mt-2 px-2 py-1 text-xs text-gray-400
                 hover:text-gray-600 dark:hover:text-gray-300 cursor-pointer"
        >
          <Plus class="w-3 h-3" /> {{ t('things.declare_field') }}
        </button>
      </section>

      <!-- Everything else on the files. Where a second word for one idea shows. -->
      <section v-if="rest.length" class="mb-7">
        <h3 class="text-[11px] font-medium uppercase tracking-wider text-gray-400 mb-2">
          {{ t('things.also_seen') }}
        </h3>
        <!--
          The same columns as the shape above, so the two sections read as one
          table — and so the difference between them is the one that matters
          rather than an accident of layout.
          
          The kind here is a fact, not a choice. A kind is part of a
          declaration and these keys have none: `fields:` in the schema *is*
          the shape, and there is nowhere to hang a kind for a key outside it.
          What is shown is what the files hold, which is what draws these
          values anyway. Adding the key to the shape is what makes it a
          decision, and it carries this kind across when it does.
        -->
        <div
          v-for="field in rest"
          :key="field.key"
          class="group grid grid-cols-[minmax(0,1fr)_88px_92px_auto] items-center gap-3
                 py-2 border-t border-gray-100 dark:border-[#232326]"
        >
          <div class="min-w-0">
            <div class="font-mono text-xs text-gray-500 dark:text-gray-400 truncate">
              {{ field.key }}
            </div>
            <div class="text-[11px] text-gray-400">
              {{ t('things.share_of_kind', { percent: share(field.count) }) }}
            </div>
          </div>
          <span class="font-mono text-xs text-gray-400 tabular-nums text-right whitespace-nowrap">
            {{ field.count }} / {{ count }}
          </span>
          <span
            v-if="observedKinds?.[field.key]"
            :title="t('things.kind_from_files')"
            class="text-[11px] text-gray-400 justify-self-start px-2 py-1 whitespace-nowrap"
          >
            {{ t(`things.kind_${observedKinds[field.key]}`) }}
          </span>
          <span v-else class="justify-self-start" />
          <!--
            A short word on the button, the whole sentence on hover.
            
            Two icons alone were unreadable — "what are these two buttons?" was
            the first thing anyone asked. The full sentence on every row was
            worse: six rows repeating "Remove from the shape (keeps the data)"
            is a wall, and the reader stops seeing any of it. So the button
            carries the verb and the tooltip carries the consequence, which is
            the part you only need once.
          -->
          <div class="flex items-center gap-1">
            <button
              type="button"
              @click="emit('adopt', field.key)"
              :title="t('things.adopt_into_shape')"
              class="px-2 py-1 rounded-md text-[11px] whitespace-nowrap cursor-pointer
                     text-gray-400 dark:text-gray-500 transition-colors
                     group-hover:text-gray-600 dark:group-hover:text-gray-300
                     hover:bg-gray-100 dark:hover:bg-white/10"
            >
              {{ t('things.adopt_short') }}
            </button>
            <!--
              Merging is offered beside the count that shows why you would want
              to. Nothing guesses which two keys mean the same thing — the
              numbers say it and a person decides. Amber because it is the only
              control on this page that touches the files.
            -->
            <button
              type="button"
              @click="emit('rename', field.key, '')"
              :title="t('things.rename_field_hint')"
              class="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] whitespace-nowrap
                     cursor-pointer text-amber-700/55 dark:text-amber-400/55 transition-colors
                     group-hover:text-amber-700 dark:group-hover:text-amber-400
                     hover:bg-amber-50 dark:hover:bg-amber-900/25"
            >
              <ArrowRight class="w-3 h-3" />
              {{ t('things.rename_short') }}
            </button>
            <button
              type="button"
              @click="emit('erase', field.key)"
              :title="t('things.delete_field_hint')"
              class="flex items-center gap-1 px-2 py-1 rounded-md text-[11px] whitespace-nowrap
                     cursor-pointer text-red-600/45 dark:text-red-400/45 transition-colors
                     group-hover:text-red-600 dark:group-hover:text-red-400
                     hover:bg-red-50 dark:hover:bg-red-900/25"
            >
              {{ t('things.erase_short') }}
            </button>
          </div>
        </div>
      </section>

      <ShapeRowMenu
        v-if="menuFor && menuAt"
        :field-key="menuFor"
        :at="menuAt"
        :can-move-up="indexOf(menuFor) > 0"
        :can-move-down="indexOf(menuFor) < usual.length - 1"
        @move="by => { emit('move', menuFor!, by); closeMenu(); }"
        @drop="emit('drop', menuFor!); closeMenu()"
        @rename="emit('rename', menuFor!, ''); closeMenu()"
        @erase="emit('erase', menuFor!); closeMenu()"
        @close="closeMenu"
      />

      <section v-if="machinery.length">
        <h3 class="text-[11px] font-medium uppercase tracking-wider text-gray-400 mb-2">
          {{ t('things.app_fields', { count: machinery.length }) }}
        </h3>
        <p class="text-[11px] text-gray-400 dark:text-gray-500 font-mono leading-relaxed">
          {{ machinery.map(f => f.key).join(' · ') }}
        </p>
      </section>
    </div>
  </div>

    <IconPicker
      v-if="iconPickerAt"
      :node-type="nodeType"
      :chosen="chosenIcon ?? null"
      :at="iconPickerAt"
      @pick="icon => { emit('pickIcon', icon); iconPickerAt = null; }"
      @close="iconPickerAt = null"
    />
</template>
