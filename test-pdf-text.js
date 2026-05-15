import fs from 'fs';
import * as pdfjs from 'pdfjs-dist';
async function test() {
  const data = new Uint8Array(fs.readFileSync('/Users/kid0604/Library/Application Support/com.synabit.app/gdrive-cache/assets/3709df17-62b2-4588-981a-837236107e96.pdf'));
  const doc = await pdfjs.getDocument(data).promise;
  const page = await doc.getPage(1);
  const textContent = await page.getTextContent();
  console.log('Page 1 items count:', textContent.items.length);
  if (textContent.items.length > 0) {
    console.log('Sample text:', textContent.items.slice(0, 5).map(i => i.str).join(' '));
  }
}
test().catch(console.error);
