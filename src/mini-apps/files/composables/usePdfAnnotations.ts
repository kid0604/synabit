import { ref, computed, type Ref } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';

export interface PdfAnnotation {
  id: string;
  nodeType: 'pdf_highlight' | 'pdf_note' | 'pdf_bookmark';
  title: string;
  content: string;  // user note (markdown)
  pdfId: string;
  pdfPath: string;
  pdfTitle: string;
  page: number;
  color: 'yellow' | 'green' | 'blue' | 'pink';
  text: string;     // highlighted text
  rects: { x: number; y: number; w: number; h: number }[];  // normalized [0,1]
  createdAt: string;
  updatedAt: string;
}

export function usePdfAnnotations(vaultPath: Ref<string>) {
  const ns = useNodeService();
  // vaultPath is kept in signature for backward compatibility but node ops use ns internally
  const annotations = ref<PdfAnnotation[]>([]);
  const isLoading = ref(false);

  /** Load all annotations for a specific PDF */
  const loadAnnotations = async (fileId: string, pdfPath: string) => {
    isLoading.value = true;
    try {
      const nodes = await ns.getNodes('pdf_highlight');
      annotations.value = nodes
        .filter(n => n.properties?.pdf_id === fileId || (!n.properties?.pdf_id && n.properties?.pdf_path === pdfPath))
        .map(nodeToAnnotation)
        .sort((a, b) => a.page - b.page || a.createdAt.localeCompare(b.createdAt));
    } catch (e) {
      console.error('Failed to load PDF annotations:', e);
      annotations.value = [];
    } finally {
      isLoading.value = false;
    }
  };

  /** Convert node from DB to PdfAnnotation */
  const nodeToAnnotation = (node: any): PdfAnnotation => ({
    id: node.id,
    nodeType: node.node_type || 'pdf_highlight',
    title: node.title,
    content: node.content || '',
    pdfId: node.properties?.pdf_id || '',
    pdfPath: node.properties?.pdf_path || '',
    pdfTitle: node.properties?.pdf_title || '',
    page: node.properties?.page || 1,
    color: node.properties?.color || 'yellow',
    text: node.properties?.text || '',
    rects: node.properties?.rects || [],
    createdAt: node.created_at || '',
    updatedAt: node.updated_at || '',
  });

  /** Create a new highlight annotation */
  const createHighlight = async (opts: {
    fileId: string;
    pdfPath: string;
    pdfTitle: string;
    page: number;
    text: string;
    rects: { x: number; y: number; w: number; h: number }[];
    color: 'yellow' | 'green' | 'blue' | 'pink';
    note?: string;
  }) => {
    const id = `PDFAnnotations/${crypto.randomUUID()}.json`;
    const title = opts.text.length > 80 ? opts.text.substring(0, 80) + '…' : opts.text;
    const properties = {
      pdf_id: opts.fileId,
      pdf_path: opts.pdfPath,
      pdf_title: opts.pdfTitle,
      page: opts.page,
      color: opts.color,
      text: opts.text,
      rects: opts.rects,
    };

    try {
      await ns.writeNode({
        relPath: id,
        title,
        nodeType: 'pdf_highlight',
        properties,
        content: opts.note || '',
        silent: true,
        eventType: 'created',
      });

      // Add to local state immediately
      annotations.value.push({
        id,
        nodeType: 'pdf_highlight',
        title,
        content: opts.note || '',
        pdfId: opts.fileId,
        pdfPath: opts.pdfPath,
        pdfTitle: opts.pdfTitle,
        page: opts.page,
        color: opts.color,
        text: opts.text,
        rects: opts.rects,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });

      // Re-sort
      annotations.value.sort((a, b) => a.page - b.page || a.createdAt.localeCompare(b.createdAt));

      return id;
    } catch (e) {
      console.error('Failed to create highlight:', e);
      throw e;
    }
  };

  /** Update annotation note */
  const updateAnnotation = async (id: string, updates: { note?: string; color?: PdfAnnotation['color']; text?: string }) => {
    const ann = annotations.value.find(a => a.id === id);
    if (!ann) return;

    const newColor = updates.color || ann.color;
    const newContent = updates.note !== undefined ? updates.note : ann.content;
    const newText = updates.text !== undefined ? updates.text : ann.text;

    try {
      await ns.writeNode({
        relPath: id,
        title: ann.title,
        nodeType: 'pdf_highlight',
        properties: {
          pdf_id: ann.pdfId,
          pdf_path: ann.pdfPath,
          pdf_title: ann.pdfTitle,
          page: ann.page,
          color: newColor,
          text: newText,
          rects: ann.rects,
        },
        content: newContent,
        silent: true,
      });

      ann.color = newColor;
      ann.content = newContent;
      ann.text = newText;
      ann.updatedAt = new Date().toISOString();
    } catch (e) {
      console.error('Failed to update annotation:', e);
    }
  };

  /** Delete an annotation */
  const deleteAnnotation = async (id: string) => {
    try {
      await ns.deleteNode({ relPath: id, silent: true });
      annotations.value = annotations.value.filter(a => a.id !== id);
    } catch (e) {
      console.error('Failed to delete annotation:', e);
    }
  };

  /** Get annotations for a specific page */
  const getPageAnnotations = (page: number) => {
    return computed(() => annotations.value.filter(a => a.page === page));
  };

  /** Export all annotations as a Markdown note */
  const exportToMarkdown = (pdfTitle: string): string => {
    if (annotations.value.length === 0) return '';

    let md = `# 📄 Annotations — ${pdfTitle}\n\n`;
    let currentPageNum = -1;

    for (const ann of annotations.value) {
      if (ann.page !== currentPageNum) {
        currentPageNum = ann.page;
        md += `## Page ${currentPageNum}\n\n`;
      }

      // Highlight color indicator
      const colorEmoji = { yellow: '🟡', green: '🟢', blue: '🔵', pink: '🩷' }[ann.color] || '🟡';
      md += `${colorEmoji} > "${ann.text}"\n\n`;

      if (ann.content.trim()) {
        md += `${ann.content}\n\n`;
      }

      md += `---\n\n`;
    }

    return md;
  };

  // ─── Drawing Management ────────────────────────────────────
  const drawings = ref<{ id: string; page: number; strokes: any[] }[]>([]);

  const loadDrawings = async (fileId: string, pdfPath: string) => {
    try {
      const nodes = await ns.getNodes('pdf_drawing');
      drawings.value = nodes
        .filter(n => n.properties?.pdf_id === fileId || (!n.properties?.pdf_id && n.properties?.pdf_path === pdfPath))
        .map(n => ({
          id: n.id,
          page: n.properties?.page || 1,
          strokes: n.properties?.strokes || []
        }));
    } catch (e) {
      console.error('Failed to load drawings:', e);
      drawings.value = [];
    }
  };

  const saveDrawing = async (fileId: string, pdfPath: string, pdfTitle: string, page: number, strokes: any[]) => {
    // Find existing drawing file for this page
    const existing = drawings.value.find(d => d.page === page);
    const id = existing ? existing.id : `PDFAnnotations/${crypto.randomUUID()}.json`;

    try {
      await ns.writeNode({
        relPath: id,
        title: `Drawing on page ${page}`,
        nodeType: 'pdf_drawing',
        properties: {
          pdf_id: fileId,
          pdf_path: pdfPath,
          pdf_title: pdfTitle,
          page,
          strokes,
        },
        content: '',
        silent: true,
      });

      if (existing) {
        existing.strokes = strokes;
      } else {
        drawings.value.push({ id, page, strokes });
      }
    } catch (e) {
      console.error('Failed to save drawing:', e);
    }
  };

  const getPageDrawingStrokes = (page: number) => {
    return computed(() => {
      const drawing = drawings.value.find(d => d.page === page);
      return drawing ? drawing.strokes : [];
    });
  };

  const clearAllAnnotations = async (pdfPath: string) => {
    const nodesToDelete = [...annotations.value.map(a => a.id), ...drawings.value.map(d => d.id)];
    if (nodesToDelete.length === 0) return;

    try {
      // Delete all corresponding files
      await Promise.all(nodesToDelete.map(id => ns.deleteNode({ relPath: id, silent: true })));
      // Reset state
      annotations.value = [];
      drawings.value = [];
    } catch (e) {
      console.error('Failed to clear annotations:', e);
    }
  };

  return {
    annotations,
    drawings,
    isLoading,
    loadAnnotations,
    loadDrawings,
    createHighlight,
    updateAnnotation,
    deleteAnnotation,
    getPageAnnotations,
    exportToMarkdown,
    saveDrawing,
    getPageDrawingStrokes,
    clearAllAnnotations,
  };
}
