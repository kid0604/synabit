import { ref, computed, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

export interface PdfAnnotation {
  id: string;
  nodeType: 'pdf_highlight' | 'pdf_note' | 'pdf_bookmark';
  title: string;
  content: string;  // user note (markdown)
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
  const annotations = ref<PdfAnnotation[]>([]);
  const isLoading = ref(false);

  /** Load all annotations for a specific PDF */
  const loadAnnotations = async (pdfPath: string) => {
    isLoading.value = true;
    try {
      const nodes = await invoke<any[]>('get_nodes', { nodeType: 'pdf_highlight' });
      annotations.value = nodes
        .filter(n => n.properties?.pdf_path === pdfPath)
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
      pdf_path: opts.pdfPath,
      pdf_title: opts.pdfTitle,
      page: opts.page,
      color: opts.color,
      text: opts.text,
      rects: opts.rects,
    };

    try {
      await invoke('write_node_file', {
        vaultPath: vaultPath.value,
        relPath: id,
        title,
        nodeType: 'pdf_highlight',
        properties,
        content: opts.note || '',
      });

      // Add to local state immediately
      annotations.value.push({
        id,
        nodeType: 'pdf_highlight',
        title,
        content: opts.note || '',
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
  const updateAnnotation = async (id: string, updates: { note?: string; color?: PdfAnnotation['color'] }) => {
    const ann = annotations.value.find(a => a.id === id);
    if (!ann) return;

    const newColor = updates.color || ann.color;
    const newContent = updates.note !== undefined ? updates.note : ann.content;

    try {
      await invoke('write_node_file', {
        vaultPath: vaultPath.value,
        relPath: id,
        title: ann.title,
        nodeType: 'pdf_highlight',
        properties: {
          pdf_path: ann.pdfPath,
          pdf_title: ann.pdfTitle,
          page: ann.page,
          color: newColor,
          text: ann.text,
          rects: ann.rects,
        },
        content: newContent,
      });

      ann.color = newColor;
      ann.content = newContent;
      ann.updatedAt = new Date().toISOString();
    } catch (e) {
      console.error('Failed to update annotation:', e);
    }
  };

  /** Delete an annotation */
  const deleteAnnotation = async (id: string) => {
    try {
      await invoke('delete_node_file', { vaultPath: vaultPath.value, relPath: id });
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

  return {
    annotations,
    isLoading,
    loadAnnotations,
    createHighlight,
    updateAnnotation,
    deleteAnnotation,
    getPageAnnotations,
    exportToMarkdown,
  };
}
