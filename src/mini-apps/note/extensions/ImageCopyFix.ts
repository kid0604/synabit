import { Extension } from '@tiptap/core';
import { Plugin, PluginKey, NodeSelection } from '@tiptap/pm/state';

export const ImageCopyFix = Extension.create({
  name: 'imageCopyFix',

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: new PluginKey('imageCopyFix'),
        props: {
          handleDOMEvents: {
            copy(view, event) {
              const { state } = view;
              const { selection } = state;

              // Check if a single image node is selected
              if (selection instanceof NodeSelection && 
                 (selection.node.type.name === 'image' || selection.node.type.name === 'customImage')) {
                
                const src = selection.node.attrs.src;
                if (src) {
                  event.preventDefault();
                  
                  // In WebKit/Safari (Tauri macOS), navigator.clipboard.write must be called synchronously 
                  // or with a ClipboardItem that takes a Promise.
                  const clipboardPromise = fetch(src)
                    .then(res => res.blob())
                    .then(blob => {
                      if (blob.type !== 'image/png') {
                        return new Promise<Blob>((resolve, reject) => {
                          const img = new Image();
                          img.onload = () => {
                            const canvas = document.createElement('canvas');
                            canvas.width = img.width;
                            canvas.height = img.height;
                            const ctx = canvas.getContext('2d');
                            if (ctx) {
                              ctx.drawImage(img, 0, 0);
                              canvas.toBlob(b => resolve(b || blob), 'image/png');
                            } else {
                              resolve(blob);
                            }
                          };
                          img.onerror = () => resolve(blob);
                          img.src = URL.createObjectURL(blob);
                        });
                      }
                      return blob;
                    });

                  try {
                    const item = new ClipboardItem({
                      'image/png': clipboardPromise
                    });
                    
                    navigator.clipboard.write([item]).then(() => {
                      // image copied successfully
                    }).catch(err => {
                      console.error('Failed to write to clipboard:', err);
                    });
                  } catch (e) {
                    console.error('Failed to create ClipboardItem:', e);
                  }
                  
                  return true;
                }
              }
              return false;
            }
          }
        }
      })
    ];
  }
});
