import { ref } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { marked } from 'marked';
import html2pdf from 'html2pdf.js';
import { writeTextFile, writeFile } from '@tauri-apps/plugin-fs';
import { logger } from '../../../utils/logger';
import type { ExportOptions } from '../NoteExportModal.vue';
import type { NoteItem } from '../helpers';

export function useNoteExport(params: {
  notes: Ref<NoteItem[]>;
  currentNoteId: Ref<string | null>;
  currentContent: ComputedRef<string>;
  vaultPath: Ref<string>;
}) {
  const { notes, currentNoteId, currentContent, vaultPath } = params;

  const exportModalVisible = ref(false);

  const convertAssetsToBase64 = async (html: string): Promise<string> => {
    if (!vaultPath.value) return html;
    const sep = vaultPath.value.includes('\\') ? '\\' : '/';
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    const imgs = doc.querySelectorAll('img');
    
    for (let img of imgs) {
        const src = img.getAttribute('src');
        if (src && src.startsWith('assets/')) {
            try {
                const decodedName = decodeURIComponent(src.substring(7));
                const absPath = `${vaultPath.value}${sep}assets${sep}${decodedName}`;
                const assetUrl = convertFileSrc(absPath);
                
                const response = await fetch(assetUrl);
                if (!response.ok) throw new Error(`Network response was not ok: ${response.statusText}`);
                const blob = await response.blob();
                
                const base64 = await new Promise<string>((resolve, reject) => {
                    const reader = new FileReader();
                    reader.onloadend = () => resolve(reader.result as string);
                    reader.onerror = reject;
                    reader.readAsDataURL(blob);
                });
                
                img.setAttribute('src', base64);
            } catch (e) {
                logger.error('Failed to convert image to base64:', src, e);
            }
        }
    }
    return doc.body.innerHTML;
  };

  const handleExportOption = async (options: ExportOptions) => {
    exportModalVisible.value = false;
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note) return;

    try {
        let defaultFileName = note.title ? note.title.replace(/[/\\?%*:|"<>]/g, '-') : 'Untitled';
        if (options.format === 'md') {
            const filePath = await save({ defaultPath: `${defaultFileName}.md`, filters: [{ name: 'Markdown', extensions: ['md'] }] });
            if (!filePath) return;
            
            let content = '';
            if (options.includeTitle) content += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) content += `Tags: ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            content += currentContent.value;
            
            await writeTextFile(filePath, content);
        } else if (options.format === 'html') {
            const filePath = await save({ defaultPath: `${defaultFileName}.html`, filters: [{ name: 'HTML', extensions: ['html'] }] });
            if (!filePath) return;
            
            let mdContent = '';
            if (options.includeTitle) mdContent += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) mdContent += `**Tags:** ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            mdContent += currentContent.value;
            
            let htmlBody = await marked.parse(mdContent);
            htmlBody = await convertAssetsToBase64(htmlBody);
            
            const htmlContent = `
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${note.title}</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; line-height: 1.6; color: #1c1c1e; padding: 2rem; max-width: 800px; margin: 0 auto; }
  h1, h2, h3, h4, h5, h6 { color: #000; font-weight: 600; margin-top: 1.5em; margin-bottom: 0.5em; }
  h1 { font-size: 2em; border-bottom: 1px solid #eaecef; padding-bottom: 0.3em; }
  a { color: #0366d6; text-decoration: none; }
  a:hover { text-decoration: underline; }
  pre { background-color: #f6f8fa; padding: 16px; overflow: auto; border-radius: 3px; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace; font-size: 85%; }
  pre code { background-color: transparent; padding: 0; border-radius: 0; font-size: 100%; }
  code { background-color: rgba(27,31,35,0.05); padding: 0.2em 0.4em; border-radius: 3px; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace; font-size: 85%; }
  blockquote { padding: 0 1em; color: #6a737d; border-left: 0.25em solid #dfe2e5; margin: 0; }
  img { max-width: 100%; height: auto; display: block; margin: 1em 0; border-radius: 4px; }
  table { border-collapse: collapse; width: 100%; margin-top: 0; margin-bottom: 16px; }
  table th, table td { padding: 6px 13px; border: 1px solid #dfe2e5; }
  table tr:nth-child(2n) { background-color: #f6f8fa; }
  ul, ol { padding-left: 2em; }
  hr { border: 0; border-bottom: 1px solid #eaecef; margin: 2em 0; }
</style>
</head>
<body>
${htmlBody}
</body>
</html>`;
            await writeTextFile(filePath, htmlContent);
        } else if (options.format === 'pdf') {
            const filePath = await save({ defaultPath: `${defaultFileName}.pdf`, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
            if (!filePath) return;
            
            let mdContent = '';
            if (options.includeTitle) mdContent += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) mdContent += `**Tags:** ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            mdContent += currentContent.value;
            
            let htmlBody = await marked.parse(mdContent);
            htmlBody = await convertAssetsToBase64(htmlBody);
            
            const container = document.createElement('div');
            container.innerHTML = htmlBody;
            container.style.padding = '20px';
            container.style.fontFamily = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif';
            container.style.color = '#1c1c1e';
            container.style.lineHeight = '1.6';
            
            const headings = container.querySelectorAll('h1, h2, h3, h4, h5, h6');
            headings.forEach((el: any) => { el.style.color = '#000'; el.style.fontWeight = '600'; el.style.marginTop = '1em'; el.style.marginBottom = '0.5em'; });
            const h1s = container.querySelectorAll('h1');
            h1s.forEach((el: any) => { el.style.fontSize = '2em'; el.style.borderBottom = '1px solid #eaecef'; el.style.paddingBottom = '0.3em'; });
            const pres = container.querySelectorAll('pre');
            pres.forEach((el: any) => { el.style.backgroundColor = '#f6f8fa'; el.style.padding = '16px'; el.style.overflow = 'auto'; el.style.borderRadius = '3px'; el.style.whiteSpace = 'pre-wrap'; el.style.fontFamily = 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace'; el.style.fontSize = '85%'; });
            const codes = container.querySelectorAll('code');
            codes.forEach((el: any) => { el.style.backgroundColor = 'rgba(27,31,35,0.05)'; el.style.padding = '0.2em 0.4em'; el.style.borderRadius = '3px'; el.style.fontFamily = 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace'; el.style.fontSize = '85%'; });
            const preCodes = container.querySelectorAll('pre code');
            preCodes.forEach((el: any) => { el.style.backgroundColor = 'transparent'; el.style.padding = '0'; el.style.borderRadius = '0'; el.style.fontSize = '100%'; });
            const blockquotes = container.querySelectorAll('blockquote');
            blockquotes.forEach((el: any) => { el.style.padding = '0 1em'; el.style.color = '#6a737d'; el.style.borderLeft = '0.25em solid #dfe2e5'; el.style.margin = '0'; });
            const imgs = container.querySelectorAll('img');
            imgs.forEach((el: any) => { el.style.maxWidth = '100%'; el.style.height = 'auto'; el.style.display = 'block'; el.style.margin = '1em 0'; el.style.borderRadius = '4px'; });
            const tables = container.querySelectorAll('table');
            tables.forEach((el: any) => { el.style.borderCollapse = 'collapse'; el.style.width = '100%'; el.style.marginBottom = '16px'; });
            const thsTds = container.querySelectorAll('th, td');
            thsTds.forEach((el: any) => { el.style.padding = '6px 13px'; el.style.border = '1px solid #dfe2e5'; });
            const hrs = container.querySelectorAll('hr');
            hrs.forEach((el: any) => { el.style.border = '0'; el.style.borderBottom = '1px solid #eaecef'; el.style.margin = '2em 0'; });
            
            document.body.appendChild(container);
            
            const opt: any = {
              margin:       10,
              filename:     defaultFileName + '.pdf',
              image:        { type: 'jpeg', quality: 0.98 },
              html2canvas:  { scale: 2, useCORS: true },
              jsPDF:        { unit: 'mm', format: options.pdfFormat, orientation: options.pdfOrientation },
              pagebreak:    { mode: ['css', 'legacy', 'avoid-all'] }
            };
            
            const pdfBlob = await html2pdf().set(opt).from(container).output('blob');
            document.body.removeChild(container);
            
            const buffer = await pdfBlob.arrayBuffer();
            const uint8Array = new Uint8Array(buffer);
            
            await writeFile(filePath, uint8Array);
        }
    } catch (e) {
        logger.error('Export failed:', e);
        alert('Export failed. Check the logs for details.');
    }
  };

  return { exportModalVisible, handleExportOption };
}
