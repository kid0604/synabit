import { createApp } from 'vue';
import PdfFileViewer from './mini-apps/files/viewers/PdfFileViewer.vue';

// Mock Tauri invoke and convertFileSrc
(window as any).__TAURI_IPC__ = async () => {};
(window as any).__TAURI__ = {
  core: {
    invoke: async (cmd: string) => {
      if (cmd === 'get_nodes') return [];
      return [];
    },
    convertFileSrc: (p: string) => '/src/assets/test.pdf'
  }
};

const app = createApp(PdfFileViewer, {
  filePath: '/src/assets/test.pdf',
  vaultPath: ''
});
app.mount('#app');
