<script setup lang="ts">
import { computed, ref, watch, nextTick, onMounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { logger } from '../../../utils/logger';
import { useDebounceFn } from '@vueuse/core';
import { marked, Renderer, type Tokens } from 'marked';
import { renderDiagram, diagramTheme } from '../../../shared/mermaid';
import { markedHighlight } from 'marked-highlight';
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import json from 'highlight.js/lib/languages/json';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import sql from 'highlight.js/lib/languages/sql';
import 'highlight.js/styles/github-dark.min.css';
import DOMPurify from 'dompurify';
import { mathExtension, MATH_ATTRS, renderMathIn } from '../markdownMath';
import 'katex/dist/katex.min.css';
import { Check, FileText, Image as ImageIcon, Wrench, ChevronDown, ChevronRight, RefreshCw, Clipboard } from 'lucide-vue-next';
import { convertFileSrc } from '@tauri-apps/api/core';
import { invoke } from '@tauri-apps/api/core';
import type { SynMessage, SourceRef } from '../types';
import FootingMark from './FootingMark.vue';
import DiagramViewer from '../../../shared/components/DiagramViewer.vue';
import { titleFor, bodyFor, KEPT_IN } from '../keepAsNote';
import { useNodeService } from '../../../composables/useNodeService';
import { useEventBus } from '../../../composables/useEventBus';
import synAvatar from '../../../assets/syn-avatar.jpg';

hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('json', json);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('sql', sql);
// `markdown` and `md` are deliberately NOT registered.
//
// highlight.js's markdown grammar hangs this renderer. A 292-character block —
// a daily-note template of headings and `- [ ]` items, which is exactly what
// the assistant produces when asked for one — froze the page indefinitely.
// The same grammar on the same input takes 2ms in Node, so this is a regex
// that backtracks catastrophically in one engine and not the other; the
// browser is the one that matters here.
//
// Established by intervention, not inference. Holding the conversation
// constant and retagging every fence: `json` renders in 38MB and stays
// responsive, no language at all renders fine, the block removed renders fine,
// and `markdown` hangs. Every arm was run twice.
//
// A size cap would not have helped — 292 characters is already small. Nor
// would a `try`/`catch`: it does not throw, it does not return. Not offering
// the grammar is the only guard that works, and the cost is that a fenced
// markdown block is shown unhighlighted, which for markdown inside markdown is
// close to no cost at all.

// Custom renderer to intercept mermaid code blocks
const renderer = new Renderer();
const originalCodeRenderer = renderer.code;
let mermaidIdCounter = 0;

renderer.code = function (token: Tokens.Code) {
  if (token.lang === 'mermaid') {
    const id = `mermaid-${Date.now()}-${mermaidIdCounter++}`;
    return `<div class="mermaid-container"><pre class="mermaid" id="${id}">${token.text}</pre></div>`;
  }
  // Fall back to original renderer for non-mermaid code
  return originalCodeRenderer.call(this, token);
};

/**
 * A code block as literal text, with the four characters that would otherwise
 * be markup taken out of play.
 *
 * `markedHighlight` treats whatever `highlight` returns as HTML, so the
 * unhighlighted path cannot hand back the source unescaped: a block containing
 * `<b>` would render as bold rather than as itself. DOMPurify would still keep
 * it safe; it would just be wrong.
 */
const asPlainCode = (code: string): string =>
  code
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

marked.use(markedHighlight({
  langPrefix: 'hljs language-',
  highlight(code, lang) {
    if (lang === 'mermaid') return code; // Don't highlight mermaid — it's handled by the renderer
    if (lang && hljs.getLanguage(lang)) {
      // `ignoreIllegals`, because a block half-written by a stream is not yet
      // valid in its own language and throwing over that is not useful.
      return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
    }

    // Deliberately no `highlightAuto`.
    //
    // It ran every registered grammar over the block to guess which language
    // it was, and that guess is both expensive and, for anything that
    // resembles no language, wrong — on a directory tree drawn with `├──` it
    // answered "css". One 3,797-character answer containing a single
    // ```text block was enough to leave the WebView unresponsive with the
    // message half-drawn, while the Rust side had been finished for sixteen
    // seconds.
    //
    // Found by bisecting a real conversation one message at a time in a
    // browser: 39 messages rendered fine, 40 hung, and the fortieth was the
    // only one in the whole conversation with a code fence.
    //
    // A block whose language nobody named is shown as what it is.
    return asPlainCode(code);
  },
}));

marked.use({ renderer });

// Mathematics. Registered after the renderer so its own `renderer` entries are
// the ones that answer for these tokens. See `markdownMath` for why all four
// delimiters are inline-level and why the TeX waits in an attribute.
marked.use(mathExtension);

const props = defineProps<{
  message: SynMessage;
  isStreaming?: boolean;
  vaultPath?: string;
}>();

const emit = defineEmits<{
  'open-source': [source: SourceRef];
  'regenerate': [];
}>();

const copied = ref(false);
const showTools = ref(false);
const fullscreenImage = ref<string | null>(null);
const fullscreenImageType = ref<'base64' | 'file'>('base64');

const openImageFullscreen = (img: string) => {
  fullscreenImage.value = img;
  fullscreenImageType.value = 'base64';
};

const openFileImageFullscreen = (path: string) => {
  fullscreenImage.value = path;
  fullscreenImageType.value = 'file';
};

const fullscreenSrc = computed(() => {
  if (!fullscreenImage.value) return '';
  return fullscreenImageType.value === 'base64'
    ? 'data:image/png;base64,' + fullscreenImage.value
    : convertFileSrc(fullscreenImage.value);
});

const formatArgs = (args: Record<string, unknown>) => {
  try {
    return JSON.stringify(args);
  } catch {
    return String(args);
  }
};

// Configure marked for clean output
marked.setOptions({
  breaks: true,
  gfm: true,
});

const debouncedContent = ref(props.message.content);

const updateContent = useDebounceFn((content: string) => {
  debouncedContent.value = content;
}, 100);

watch(() => props.message.content, (newContent) => {
  if (props.isStreaming) {
    updateContent(newContent);
  } else {
    debouncedContent.value = newContent;
  }
}, { immediate: true });

const renderedContent = computed(() => {
  if (props.message.role === 'user') {
    return debouncedContent.value;
  }

  const rawHtml = marked.parse(debouncedContent.value) as string;
  let sanitized = DOMPurify.sanitize(rawHtml, {
    ADD_TAGS: ['pre', 'code', 'svg', 'g', 'path', 'rect', 'circle', 'line', 'polyline', 'polygon', 'text', 'tspan', 'defs', 'clipPath', 'use', 'marker', 'foreignObject', 'style'],
    ADD_ATTR: ['class', 'id', 'viewBox', 'xmlns', 'd', 'fill', 'stroke', 'stroke-width', 'transform', 'x', 'y', 'width', 'height', 'rx', 'ry', 'cx', 'cy', 'r', 'x1', 'y1', 'x2', 'y2', 'points', 'text-anchor', 'dominant-baseline', 'font-size', 'font-weight', 'font-family', 'opacity', 'clip-path', 'marker-end', 'marker-start', 'style', 'dx', 'dy', 'alignment-baseline', 'data-wikilink', ...MATH_ATTRS],
  });

  // Convert [[Title]] wiki-links to clickable links
  sanitized = sanitized.replace(
    /\[\[([^\]]+)\]\]/g,
    (_match, title) => `<a class="wikilink" data-wikilink="${title.replace(/"/g, '&quot;')}" href="#">${title}</a>`
  );

  return sanitized;
});

// Trigger mermaid rendering after content updates
const messageEl = ref<HTMLElement | null>(null);

/**
 * The rendered diagrams, kept so one can be opened big.
 *
 * The SVG goes into the bubble through `innerHTML` and is then the DOM's, not
 * this component's — reading it back out of the DOM to show it again would work
 * and would be reading a copy of something already held. Keyed by the id the
 * renderer minted, which is what the click carries.
 */
const diagrams = new Map<string, string>();
/** And what each was drawn from, so a theme change can draw it again. */
const sources = new Map<string, string>();
/** And where one has been kept, so the button can offer to open it. */
const keptNotes = new Map<string, SourceRef>();
const openDiagram = ref<string | null>(null);

// The template uses the global `$t`; the label below is written into markup
// from script, so it needs the composable.
const { t } = useI18n();
const nodes = useNodeService();
const bus = useEventBus();

const renderMermaid = async () => {
  await nextTick();
  if (!messageEl.value) return;
  
  const mermaidEls = messageEl.value.querySelectorAll('pre.mermaid:not([data-processed])');
  if (mermaidEls.length === 0) return;

  for (const el of mermaidEls) {
    const id = el.id || `mermaid-auto-${Date.now()}`;
    const code = el.textContent || '';
    if (!code.trim()) continue;

    const drawn = await renderDiagram(id + '-svg', code);
    if ('error' in drawn) {
      console.warn('[Mermaid] Render failed:', drawn.error);
      el.setAttribute('data-processed', 'error');
      continue;
    }

    {
      const { svg } = drawn;
      diagrams.set(id, svg);
      sources.set(id, code);
      // `data-diagram` is what `handleContentClick` looks for, and the button
      // role plus the label are what make a picture that does something say so
      // to somebody who cannot see the cursor change.
      // The diagram, and beneath it what can be done with it.
      //
      // The actions sit *outside* the `role="button"` that opens the viewer:
      // a button inside a button is a click nobody can predict and a thing no
      // screen reader can describe. And they are the app's own — never
      // something the model asked for. See `keepAsNote`.
      const opened =
        `<div class="mermaid-rendered" data-diagram="${id}" role="button" tabindex="0"` +
        ` title="${t('syn.diagram_open')}">${svg}</div>` +
        `<div class="mermaid-actions"><button type="button" data-act="keep"` +
        ` data-for="${id}">${t('syn.keep_as_note')}</button></div>`;
      const container = el.parentElement;
      if (container && container.classList.contains('mermaid-container')) {
        container.innerHTML = opened;
      } else {
        el.outerHTML = opened;
      }
    }
  }
};

/**
 * Draw them again in the other theme.
 *
 * A diagram is an SVG with its colours baked into it, so switching the app
 * between light and dark is a re-render rather than a stylesheet change. Which
 * is why the source is kept beside the picture: there is nothing left in the
 * DOM to derive it from once the `<pre>` has been replaced.
 */
watch(diagramTheme, async () => {
  if (!messageEl.value) return;
  for (const [id, code] of sources) {
    const drawn = await renderDiagram(id + '-svg', code);
    if ('error' in drawn) continue;
    diagrams.set(id, drawn.svg);
    const held = messageEl.value.querySelector(`[data-diagram="${id}"]`);
    if (held) held.innerHTML = drawn.svg;
  }
});

/**
 * Turn the parked formulas into mathematics.
 *
 * # Why this one runs while the answer is still arriving and Mermaid does not
 *
 * Because a half-written formula is not a formula at all — the tokenizer needs
 * both delimiters, so an unfinished one stays as the text it already is and
 * becomes mathematics the moment it closes. A half-written *diagram* is a
 * syntax error, and re-rendering one on every chunk is a stream of red boxes.
 *
 * It is not free, and the figure is measured rather than assumed —
 * `streamRenderCost` prints it. On an answer that is almost nothing but
 * mathematics, eighteen formulas in 933 characters cost KaTeX **6.9ms a pass**
 * against 0.04ms for the markdown around them. At the 100ms debounce that is
 * roughly 7% of one core while such an answer streams, and it is a ceiling
 * rather than a typical message: an answer with no mathematics in it pays one
 * `querySelectorAll` that finds nothing.
 *
 * The alternative was showing `$$\int u\,dv$$` for the whole of a stream and
 * then snapping to mathematics at the end, which is the thing being fixed,
 * arriving late.
 */
const renderMath = async () => {
  await nextTick();
  if (messageEl.value) renderMathIn(messageEl.value);
};

watch(renderedContent, () => {
  if (props.message.role !== 'assistant') return;
  renderMath();
  if (!props.isStreaming) renderMermaid();
});

// Also render when streaming finishes
watch(() => props.isStreaming, (streaming, wasStreaming) => {
  if (wasStreaming && !streaming) {
    renderMath();
    renderMermaid();
  }
});

onMounted(() => {
  if (props.message.role === 'assistant' && !props.isStreaming) {
    renderMath();
    renderMermaid();
  }
});

const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico'];

/**
 * Do what a block's own button says.
 *
 * Two states, one button. **Keep** writes the note and the button becomes
 * **open it** — rather than a toast that fades, or a jump to Notes that takes
 * the reader out of the conversation they are in. Keeping something and going
 * to look at it are two decisions, and only the first one was made.
 *
 * A failure leaves the button as it was and says so on it. There is no toast
 * system here to say it in, and a control that silently does nothing is worse
 * than one that admits it.
 */
const runBlockAction = async (button: HTMLElement) => {
  const id = button.dataset.for ?? '';

  if (button.dataset.act === 'open') {
    const kept = keptNotes.get(id);
    if (!kept) return;

    // Asked for before going there, because the note can be gone by a route
    // this window never saw: deleted on another device and synced in, or in a
    // session before this one. The bus below catches the case where it
    // happened here; this catches the rest.
    //
    // Without it the reader is handed to an editor opening a file that is not
    // there, which does not fail — it waits, forever, on a spinner.
    const still = await nodes.getNode(kept.id).catch(() => null);
    if (!still) {
      forgetKept(id);
      return;
    }

    emit('open-source', kept);
    return;
  }

  if (button.dataset.act !== 'keep') return;
  const code = sources.get(id);
  if (!code) return;

  button.setAttribute('disabled', 'true');
  try {
    const title = titleFor(code, props.message.content ?? '', t('syn.keep_untitled'));
    const relPath = await nodes.createNode({ directory: KEPT_IN, nodeType: 'note', silent: true });
    await nodes.writeNode({
      relPath,
      nodeType: 'note',
      title,
      // Nothing to say about any field. Named keys are set and unnamed ones
      // are left as they are, so an empty object keeps whatever
      // `create_node_file` just wrote into the frontmatter.
      properties: {},
      content: bodyFor(code),
      eventType: 'created',
    });

    keptNotes.set(id, { id: relPath, title, node_type: 'note' });
    button.dataset.act = 'open';
    button.textContent = t('syn.keep_open_it', { title });
  } catch (e) {
    logger.error('[Syn] Could not keep the diagram', e);
    button.textContent = t('syn.keep_failed');
  } finally {
    button.removeAttribute('disabled');
  }
};

/**
 * Put the button back to offering to keep it.
 *
 * A control that claims a note exists when it does not is worse than no
 * control: it is an invitation into a dead end. Keeping is still possible —
 * the diagram has not gone anywhere — so the button goes back to saying so.
 */
const forgetKept = (id: string) => {
  keptNotes.delete(id);
  const button = messageEl.value?.querySelector(`[data-act][data-for="${id}"]`);
  if (!(button instanceof HTMLElement)) return;
  button.dataset.act = 'keep';
  button.textContent = t('syn.keep_as_note');
};

/**
 * A note kept from here, deleted anywhere.
 *
 * The vault is one thing and this panel is a view of it; a note trashed from
 * the Notes sidebar is the same note this button is pointing at.
 */
bus.on('node:deleted', ({ id }) => {
  for (const [diagram, kept] of keptNotes) {
    if (kept.id === id) forgetKept(diagram);
  }
});

/** Open the diagram this element sits in, if it sits in one. */
const showDiagramUnder = (target: HTMLElement): boolean => {
  const diagram = target.closest('[data-diagram]') as HTMLElement | null;
  if (!diagram) return false;
  const svg = diagrams.get(diagram.dataset.diagram ?? '');
  if (svg) openDiagram.value = svg;
  return true;
};

/**
 * The same, from the keyboard.
 *
 * The diagram carries `role="button"`, and a button that only answers a mouse
 * is a lie told to whoever is reading this with a keyboard or a screen reader.
 */
const handleContentKey = (e: KeyboardEvent) => {
  if (e.key !== 'Enter' && e.key !== ' ') return;
  if (showDiagramUnder(e.target as HTMLElement)) e.preventDefault();
};

/** Handle clicks on wiki-links [[Title]] in rendered content */
const handleContentClick = async (e: MouseEvent) => {
  const target = e.target as HTMLElement;

  // An action on a block comes first: it sits inside the same container as the
  // diagram, and a diagram that opened as well would open behind the answer.
  const act = target.closest('[data-act]') as HTMLElement | null;
  if (act) {
    e.preventDefault();
    void runBlockAction(act);
    return;
  }

  // A diagram fitted into a four-hundred-pixel bubble is a picture of a
  // diagram. Clicking it opens the one you can read.
  if (showDiagramUnder(target)) {
    e.preventDefault();
    return;
  }

  const link = target.closest('a.wikilink') as HTMLElement | null;
  if (!link) return;
  
  e.preventDefault();
  e.stopPropagation();
  
  const linkText = link.dataset.wikilink;
  if (!linkText) return;

  try {
    // Check if the link text looks like a file path (e.g., "Notes/UUID.md")
    const isPath = linkText.includes('/') || linkText.endsWith('.md') || linkText.endsWith('.json');
    
    if (isPath) {
      // Try to open directly by ID (path)
      try {
        const node = await invoke<{ id: string; item_type: string; title: string }>(
          'get_nexus_item',
          { vaultPath: props.vaultPath || '', id: linkText }
        );
        emit('open-source', {
          id: node.id,
          title: node.title,
          node_type: node.item_type,
        });
        return;
      } catch {
        // If direct lookup fails, fall through to search
      }
    }
    
    // Search for the node by title
    const result = await invoke<{ results: Array<{ id: string; item_type: string; title: string }> }>(
      'search_nexus', 
      { vaultPath: props.vaultPath || '', query: `in:title "${linkText}"` }
    );
    
    if (result.results && result.results.length > 0) {
      // Find exact title match first, fall back to first result
      const exactMatch = result.results.find(
        r => r.title.toLowerCase() === linkText.toLowerCase()
      ) || result.results[0];
      
      emit('open-source', {
        id: exactMatch.id,
        title: exactMatch.title,
        node_type: exactMatch.item_type,
      });
    } else {
      console.warn(`[WikiLink] No node found for: ${linkText}`);
    }
  } catch (err) {
    console.error('[WikiLink] Failed:', err);
  }
};
const VIDEO_EXTENSIONS = ['mp4', 'mov', 'webm', 'avi', 'mkv'];
const MEDIA_EXTENSIONS = [...IMAGE_EXTENSIONS, ...VIDEO_EXTENSIONS];

/** Extract media file paths from search_files tool call results */
const fileMediaPreviews = ref<{ path: string; filename: string; type: 'image' | 'video' }[]>([]);

watch(() => props.message.tool_calls_log, (newLogs) => {
  if (props.message.role !== 'assistant' || !newLogs?.length) {
    if (fileMediaPreviews.value.length > 0) {
      fileMediaPreviews.value = [];
    }
    return;
  }
  
  const media: { path: string; filename: string; type: 'image' | 'video' }[] = [];
  
  for (const tc of newLogs) {
    if (tc.tool_name !== 'search_files') continue;
    
    const preview = tc.result_preview;
    
    // Try JSON.parse first (most reliable)
    try {
      const data = JSON.parse(preview);
      if (data.results && Array.isArray(data.results)) {
        for (const r of data.results) {
          const ext = (r.extension || '').toLowerCase();
          if (ext && MEDIA_EXTENSIONS.includes(ext) && r.path) {
            media.push({
              path: r.path,
              filename: r.filename || r.path.split('/').pop() || '',
              type: VIDEO_EXTENSIONS.includes(ext) ? 'video' : 'image',
            });
          }
        }
        continue;
      }
    } catch {
      // JSON truncated — fall back to regex
    }
    
    // Regex fallback for truncated JSON
    try {
      const pathMatches = preview.matchAll(/"path"\s*:\s*"([^"]+)"/g);
      const extMatches = preview.matchAll(/"extension"\s*:\s*"([^"]+)"/g);
      const nameMatches = preview.matchAll(/"filename"\s*:\s*"([^"]+)"/g);
      
      const paths = [...pathMatches].map(m => m[1]);
      const exts = [...extMatches].map(m => m[1].toLowerCase());
      const names = [...nameMatches].map(m => m[1]);
      
      for (let i = 0; i < paths.length; i++) {
        if (exts[i] && MEDIA_EXTENSIONS.includes(exts[i])) {
          media.push({
            path: paths[i],
            filename: names[i] || paths[i].split('/').pop() || '',
            type: VIDEO_EXTENSIONS.includes(exts[i]) ? 'video' : 'image',
          });
        }
      }
    } catch {
      // Ignore parse errors
    }
  }
  
  fileMediaPreviews.value = media;
}, { immediate: true, deep: true });

/** Open a local file using the OS default viewer */
const openFileWithOS = async (path: string) => {
  try {
    await invoke('open_local_file', { 
      vaultPath: props.vaultPath || '', 
      path 
    });
  } catch (e) {
    console.error('Failed to open file:', e);
  }
};

const formatTime = (iso: string) => {
  const d = new Date(iso);
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

const copyContent = async () => {
  try {
    await navigator.clipboard.writeText(props.message.content);
    copied.value = true;
    setTimeout(() => { copied.value = false; }, 2000);
  } catch {
    // Ignore clipboard errors
  }
};
</script>

<template>
  <div
    class="message-bubble flex gap-3 w-full"
    :class="message.role === 'user' ? 'flex-row-reverse' : 'flex-row'"
    style="animation: messageIn 0.25s ease-out forwards;"
  >
    <!-- Avatar (assistant only) -->
    <div
      v-if="message.role === 'assistant'"
      class="w-8 h-8 rounded-xl overflow-hidden flex-shrink-0 mt-0.5 ring-1 ring-violet-500/30 shadow-lg shadow-violet-500/20"
    >
      <img :src="synAvatar" alt="Syn" class="w-full h-full object-cover" />
    </div>

    <!--
      Content.

      A question keeps to 80% and stays on its side of the column. An answer
      takes the column, because an answer is where the tables and the diagrams
      are — and capping it at 80% of a wider column would put the width back
      where it was and leave the gap where it was too. What keeps an answer
      *readable* at that width is the measure on prose inside it, not a cap on
      the card.
    -->
    <div
      class="group relative min-w-0"
      :class="message.role === 'user' ? 'flex flex-col items-end max-w-[80%]' : 'flex-1'"
    >
      <!-- Bubble -->
      <div
        class="px-4 py-3 rounded-2xl text-sm leading-relaxed relative overflow-hidden select-text"
        :class="message.role === 'user'
          ? 'bg-gradient-to-r from-violet-500 to-purple-600 text-white rounded-br-md shadow-lg shadow-violet-500/20'
          : 'bg-white dark:bg-[#1e1f25] border border-gray-100 dark:border-gray-800/60 rounded-tl-md shadow-sm'"
      >
        <!-- User message -->
        <template v-if="message.role === 'user'">
          <div class="whitespace-pre-wrap break-words">
            {{ message.content }}
          </div>
          
          <!-- Attached images -->
          <div v-if="message.images?.length" class="flex flex-wrap gap-2 mt-2">
            <img
              v-for="(img, i) in message.images"
              :key="i"
              :src="'data:image/png;base64,' + img"
              class="max-w-52 max-h-52 rounded-lg object-cover cursor-pointer
                     border border-white/20
                     hover:border-white/50 transition-colors"
              @click="openImageFullscreen(img)"
            />
          </div>
        </template>

        <!-- Assistant message: rendered markdown -->
        <div
          v-else
          ref="messageEl"
          class="prose prose-sm dark:prose-invert max-w-none
            prose-p:my-1.5 prose-p:leading-relaxed
            prose-pre:bg-gray-900 prose-pre:text-gray-100 prose-pre:rounded-lg prose-pre:my-3
            prose-code:text-violet-600 dark:prose-code:text-violet-400
            prose-headings:font-semibold prose-headings:mt-4 prose-headings:mb-2
            prose-ul:my-2 prose-ol:my-2 prose-li:my-0.5
            prose-a:text-violet-600 dark:prose-a:text-violet-400 prose-a:no-underline hover:prose-a:underline
            prose-blockquote:border-violet-300 dark:prose-blockquote:border-violet-600
            prose-strong:text-gray-900 dark:prose-strong:text-white"
          v-html="renderedContent"
          @click="handleContentClick"
          @keydown="handleContentKey"
        />

        <!-- Streaming cursor -->
        <span
          v-if="isStreaming && message.role === 'assistant'"
          class="inline-block w-0.5 h-4 bg-violet-500 ml-0.5 align-middle streaming-cursor"
        />

        <!-- Tool Calls Log (collapsible) -->
        <div v-if="message.role === 'assistant' && message.tool_calls_log?.length" class="mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50">
          <button
            @click="showTools = !showTools"
            class="flex items-center gap-1.5 text-[11px] font-medium text-gray-500 dark:text-gray-400 
                   hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <Wrench class="w-3 h-3" />
            <span>{{ message.tool_calls_log.length }} tool call{{ message.tool_calls_log.length > 1 ? 's' : '' }}</span>
            <ChevronDown v-if="showTools" class="w-3 h-3" />
            <ChevronRight v-else class="w-3 h-3" />
          </button>
          
          <Transition
            enter-active-class="transition-all duration-200 ease-out"
            enter-from-class="max-h-0 opacity-0"
            enter-to-class="max-h-96 opacity-100"
            leave-active-class="transition-all duration-150 ease-in"
            leave-from-class="max-h-96 opacity-100"
            leave-to-class="max-h-0 opacity-0"
          >
            <div v-if="showTools" class="mt-2 space-y-1 overflow-hidden">
              <div
                v-for="(tc, i) in message.tool_calls_log"
                :key="i"
                class="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-gray-50 dark:bg-gray-800/30 text-[11px]"
              >
                <span class="text-violet-500 font-mono font-medium shrink-0">{{ tc.tool_name }}</span>
                <span class="text-gray-400 font-mono truncate">{{ formatArgs(tc.tool_args) }}</span>
                <span v-if="tc.result_preview" class="text-gray-500 dark:text-gray-400 truncate ml-auto">→ {{ tc.result_preview.slice(0, 80) }}</span>
              </div>
            </div>
          </Transition>
        </div>

        <!-- File Media Previews (when search_files found images/videos) -->
        <div v-if="fileMediaPreviews.length" 
             class="mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50">
          <div class="flex items-center gap-1.5 mb-2 text-[11px] font-medium text-gray-500 dark:text-gray-400">
            <ImageIcon class="w-3 h-3" />
            <span>{{ fileMediaPreviews.length }} file{{ fileMediaPreviews.length > 1 ? 's' : '' }}</span>
          </div>
          <div class="flex flex-wrap gap-2">
            <div
              v-for="(media, i) in fileMediaPreviews"
              :key="i"
              class="group/file relative cursor-pointer"
              @click="media.type === 'image' ? openFileImageFullscreen(media.path) : openFileWithOS(media.path)"
              @dblclick.stop="openFileWithOS(media.path)"
            >
              <!-- Image preview -->
              <img
                v-if="media.type === 'image'"
                :src="convertFileSrc(media.path)"
                :alt="media.filename"
                class="w-28 h-28 rounded-lg object-cover border border-gray-200 dark:border-gray-700
                       hover:border-violet-400 dark:hover:border-violet-500 transition-all
                       hover:shadow-lg hover:shadow-violet-500/10"
              />
              <!-- Video preview -->
              <div v-else class="w-28 h-28 rounded-lg border border-gray-200 dark:border-gray-700
                                 hover:border-violet-400 dark:hover:border-violet-500 transition-all
                                 hover:shadow-lg hover:shadow-violet-500/10
                                 bg-gray-900 flex items-center justify-center relative overflow-hidden">
                <video
                  :src="convertFileSrc(media.path)"
                  class="w-full h-full object-cover absolute inset-0"
                  muted preload="metadata"
                />
                <div class="absolute inset-0 bg-black/30 flex items-center justify-center">
                  <div class="w-8 h-8 rounded-full bg-white/90 flex items-center justify-center">
                    <svg class="w-4 h-4 text-gray-800 ml-0.5" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M8 5v14l11-7z"/>
                    </svg>
                  </div>
                </div>
              </div>
              <!-- Filename overlay -->
              <div class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/70 to-transparent 
                          rounded-b-lg px-1.5 py-1 opacity-0 group-hover/file:opacity-100 transition-opacity">
                <span class="text-[9px] text-white truncate block">{{ media.filename }}</span>
              </div>
            </div>
          </div>
        </div>

        <!-- What this answer stood on, and what it can point at. One rule
             above both, because they are the same statement at two levels of
             detail: the mark says whether there is a source, the chips say
             which. See `FootingMark.vue`. -->
        <div
          v-if="message.role === 'assistant' && (message.footing || message.sources?.length)"
          class="mt-3 pt-3 border-t border-gray-100 dark:border-gray-800/50 space-y-2"
        >
          <FootingMark v-if="message.footing" :footing="message.footing" />

          <div v-if="message.sources?.length" class="flex flex-wrap gap-1.5">
          <button
            v-for="source in message.sources"
            :key="source.id"
            @click="$emit('open-source', source)"
            class="inline-flex items-center gap-1 px-2 py-0.5 text-[11px] font-medium rounded-md 
                   bg-violet-50 dark:bg-violet-900/20 text-violet-600 dark:text-violet-400 
                   hover:bg-violet-100 dark:hover:bg-violet-900/40 transition-colors cursor-pointer"
          >
            <FileText class="w-3 h-3" />
            {{ source.title }}
          </button>
          </div>
        </div>
      </div>

      <!-- Hover action bar -->
      <div class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity duration-150
                  flex items-center gap-0.5 bg-white dark:bg-gray-800 rounded-lg shadow-md border border-gray-100 dark:border-gray-700/50 px-1 py-0.5">
        <button
          @click="copyContent"
          class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          :title="copied ? $t('syn.copied') : $t('syn.copy')"
        >
          <Check v-if="copied" class="w-3.5 h-3.5 text-green-500" />
          <Clipboard v-else class="w-3.5 h-3.5" />
        </button>
        <button
          v-if="message.role === 'assistant'"
          @click="$emit('regenerate')"
          class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          :title="$t('syn.regenerate')"
        >
          <RefreshCw class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- Metadata row -->
      <div
        class="flex items-center gap-2 mt-1 px-1 opacity-0 group-hover:opacity-100 transition-opacity duration-150"
        :class="message.role === 'user' ? 'flex-row-reverse' : ''"
      >
        <span class="text-[11px] text-gray-400 dark:text-gray-500">
          {{ formatTime(message.timestamp) }}
        </span>
        <span
          v-if="message.role === 'assistant' && (message.tokens || message.duration_ms)"
          class="text-[11px] text-gray-400 dark:text-gray-500"
        >
          <template v-if="message.tokens">{{ message.tokens }} {{ $t('syn.tokens') }}</template>
          <template v-if="message.tokens && message.duration_ms"> · </template>
          <template v-if="message.duration_ms">{{ (message.duration_ms / 1000).toFixed(1) }}s</template>
        </span>
      </div>
    </div>
  </div>

  <DiagramViewer :svg="openDiagram" @close="openDiagram = null" />

  <!-- Fullscreen image lightbox -->
  <Teleport to="body">
    <Transition
      enter-active-class="transition-opacity duration-200 ease-out"
      enter-from-class="opacity-0"
      enter-to-class="opacity-100"
      leave-active-class="transition-opacity duration-150 ease-in"
      leave-from-class="opacity-100"
      leave-to-class="opacity-0"
    >
      <div
        v-if="fullscreenImage"
        class="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center cursor-pointer"
        @click="fullscreenImage = null"
      >
        <img
          :src="fullscreenSrc"
          class="max-w-[90vw] max-h-[90vh] rounded-xl object-contain shadow-2xl"
          @click.stop
        />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@keyframes messageIn {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0; }
}

.streaming-cursor {
  animation: blink 0.8s ease-in-out infinite;
}

/* Code block styling for assistant messages */
:deep(pre) {
  position: relative;
  margin: 0.75rem 0;
  border-radius: 0.75rem;
  overflow-x: auto;
}

:deep(pre code) {
  display: block;
  padding: 1rem;
  font-size: 0.8rem;
  line-height: 1.6;
  font-family: 'JetBrains Mono', 'Fira Code', 'Menlo', monospace;
}

:deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 0.5rem 0;
}

:deep(th), :deep(td) {
  border: 1px solid var(--color-border);
  padding: 0.4rem 0.75rem;
  text-align: left;
  font-size: 0.8rem;
}

:deep(th) {
  background-color: var(--color-surface-hover);
  font-weight: 600;
}

.dark :deep(th) {
  background-color: var(--color-surface-hover-dark);
}

.dark :deep(th), .dark :deep(td) {
  border-color: var(--color-border-dark);
}

.dark :deep(.mermaid-rendered),
.dark :deep(pre.mermaid) {
  background: rgba(30, 31, 37, 0.5);
}

/*
  The measure.

  A line of prose stops being comfortable somewhere around seventy-five
  characters, and the column an answer now gets is wider than that on purpose —
  so the width goes to the things that need it and the sentences keep the limit
  they already had.

  **76ch is not a narrowing.** At this font size it is about 590 pixels, and a
  paragraph in this panel was 582: 80% of a 48rem column, less the card's
  padding. Prose reads exactly as it did; only the room around it changed.

  Applied to the text-level children only. A table, a diagram, a code block or
  an image is *why* the column is wide, and capping those would undo the whole
  change. In `ch` rather than pixels because the limit is about characters per
  line, which is the thing `ch` measures.
*/
:deep(.prose > p),
:deep(.prose > ul),
:deep(.prose > ol),
:deep(.prose > blockquote),
:deep(.prose > h1),
:deep(.prose > h2),
:deep(.prose > h3),
:deep(.prose > h4) {
  max-width: 76ch;
}

/*
  And a table that is wider than the column scrolls rather than crushing its
  columns into one word each. The column is wide now; this is the backstop for
  the tables that are wider still.
*/
:deep(.prose table) {
  display: block;
  width: fit-content;
  max-width: 100%;
  overflow-x: auto;
}

/* Mermaid chart containers */
:deep(.mermaid-container) {
  margin: 0.75rem 0;
  /* Column, because the diagram now has a row of its own controls under it. */
  display: flex;
  flex-direction: column;
}

:deep(.mermaid-rendered) {
  display: flex;
  justify-content: center;
  /*
    The card follows the app, because the diagram inside it does now.
    It used to be this dark in both themes, which is why a diagram in a light
    conversation sat in a grey slab — the one visible trace of the chat having
    its own private Mermaid configuration.
  */
  background: rgba(0, 0, 0, 0.03);
  border-radius: 0.75rem;
  padding: 1rem;
  border: 1px solid rgba(124, 58, 237, 0.15);
  overflow-x: auto;
  /*
    It opens. `zoom-in` says so before anything is clicked, which matters more
    here than anywhere else in this bubble: a diagram squeezed to bubble width
    looks like a picture that has already given you everything it has.
  */
  cursor: zoom-in;
  transition: border-color 0.15s ease;
}

/*
  What can be done with the block, under the block.

  Quiet until the diagram is hovered: twenty-nine answers each carrying a
  visible button is a wall of controls, and the one that matters is the one
  under the thing you are already looking at. It stays visible once focused, or
  it could not be reached from a keyboard at all.
*/
:deep(.mermaid-actions) {
  display: flex;
  justify-content: flex-end;
  margin-top: 0.25rem;
  opacity: 0;
  transition: opacity 0.15s ease;
}

:deep(.mermaid-container:hover .mermaid-actions),
:deep(.mermaid-actions:focus-within) {
  opacity: 1;
}

:deep(.mermaid-actions button) {
  font-size: 11px;
  padding: 0.2rem 0.55rem;
  border-radius: 0.4rem;
  color: rgb(109 40 217);
  background: rgba(124, 58, 237, 0.08);
  cursor: pointer;
  transition: background 0.15s ease;
}

:deep(.mermaid-actions button:hover) {
  background: rgba(124, 58, 237, 0.16);
}

:deep(.mermaid-actions button[disabled]) {
  opacity: 0.5;
  cursor: default;
}

.dark :deep(.mermaid-actions button) {
  color: rgb(196 181 253);
  background: rgba(124, 58, 237, 0.18);
}

:deep(.mermaid-rendered:hover) {
  border-color: rgba(124, 58, 237, 0.45);
}

:deep(.mermaid-rendered:focus-visible) {
  outline: 2px solid rgba(124, 58, 237, 0.6);
  outline-offset: 2px;
}

:deep(.mermaid-rendered svg) {
  max-width: 100%;
  height: auto;
}

/*
  Mathematics.

  A display formula gets room around it and scrolls rather than overflowing:
  a long derivation is wider than a chat bubble, and a bubble that grows to fit
  one pushes every other message sideways.
*/
:deep(.syn-math-display) {
  display: block;
  margin: 0.75rem 0;
  overflow-x: auto;
  overflow-y: hidden;
  padding-bottom: 0.25rem;
}

/* KaTeX draws its own errors in red when it cannot parse. Left visible on
   purpose — it tells the reader, and whoever wrote the prompt, something. */
:deep(.katex-error) {
  font-family: ui-monospace, monospace;
  font-size: 0.85em;
}

:deep(pre.mermaid) {
  background: rgba(0, 0, 0, 0.03);
  border-radius: 0.75rem;
  padding: 1rem;
  border: 1px solid rgba(124, 58, 237, 0.15);
  color: #a78bfa;
  font-size: 0.8rem;
  white-space: pre-wrap;
}

/* Wiki-link [[Title]] styling */
:deep(a.wikilink) {
  color: #a78bfa;
  background: rgba(124, 58, 237, 0.1);
  padding: 0.1em 0.4em;
  border-radius: 0.25rem;
  text-decoration: none;
  cursor: pointer;
  transition: all 0.15s ease;
  font-weight: 500;
}

:deep(a.wikilink:hover) {
  background: rgba(124, 58, 237, 0.25);
  color: #c4b5fd;
  text-decoration: none;
}

:deep(a.wikilink::before) {
  content: '📄 ';
  font-size: 0.75em;
}
</style>
