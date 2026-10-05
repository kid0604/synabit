/**
 * A one-page PDF holding one picture.
 *
 * What a board exported as PDF is: the same picture as the PNG export, on a
 * page its own size, so it prints and attaches like any document. Written by
 * hand because that is all it takes — a JPEG can be placed in a PDF as it is
 * (the format reads JPEG natively), so there is nothing to encode and no
 * library to carry for it.
 */
/**
 * The size, in pixels, a JPEG says it is: read from its frame header.
 *
 * The capture's own size is not the number asked for — the canvas is cut to
 * whole pixels, and shrunk when it would be bigger than the webview draws —
 * and a PDF that declares a different size than the picture holds is one a
 * reader may refuse or draw wrong. Null when there is no frame header.
 */
export function jpegSize(jpeg: Uint8Array): { width: number; height: number } | null {
  if (jpeg.length < 4 || jpeg[0] !== 0xff || jpeg[1] !== 0xd8) return null;
  let i = 2;
  while (i + 3 < jpeg.length) {
    if (jpeg[i] !== 0xff) return null;
    let marker = jpeg[i + 1];
    // Any number of 0xFF may pad before a marker.
    while (marker === 0xff && i + 2 < jpeg.length) marker = jpeg[++i + 1];
    i += 2;
    // Markers that stand alone, with no length after them.
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd8)) continue;
    if (marker === 0xd9 || marker === 0xda) return null; // the image data begins: no frame header came first
    if (i + 1 >= jpeg.length) return null;
    const length = (jpeg[i] << 8) | jpeg[i + 1];
    // Start of frame: every SOFn but DHT (C4), JPG (C8) and DAC (CC).
    if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
      if (i + 6 >= jpeg.length) return null;
      const height = (jpeg[i + 3] << 8) | jpeg[i + 4];
      const width = (jpeg[i + 5] << 8) | jpeg[i + 6];
      return width && height ? { width, height } : null;
    }
    if (length < 2) return null;
    i += length;
  }
  return null;
}

/** The largest a page may be on either side, in points: PDF 1.4's implementation limit. */
export const MAX_PAGE_SIDE = 14400;

/** A page size, shrunk to fit within `MAX_PAGE_SIDE` with its shape kept. */
export function fitPage(width: number, height: number, max = MAX_PAGE_SIDE): { width: number; height: number } {
  const scale = Math.min(1, max / width, max / height);
  return { width: width * scale, height: height * scale };
}

/**
 * `pixelWidth` and `pixelHeight` are used only when the JPEG does not say its
 * own size. The page is shrunk to fit PDF's limit; the picture keeps every
 * pixel it has.
 */
export function jpegToPdf(jpeg: Uint8Array, pixelWidth: number, pixelHeight: number, pageWidth: number, pageHeight: number): Uint8Array {
  const pixels = jpegSize(jpeg) ?? { width: Math.round(pixelWidth), height: Math.round(pixelHeight) };
  const page = fitPage(pageWidth, pageHeight);
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  const offsets: number[] = [];
  let length = 0;
  const push = (chunk: Uint8Array | string) => {
    const bytes = typeof chunk === 'string' ? enc.encode(chunk) : chunk;
    parts.push(bytes);
    length += bytes.length;
  };
  const object = (n: number, body: Uint8Array | string, stream?: Uint8Array) => {
    offsets[n] = length;
    push(`${n} 0 obj\n`);
    push(body);
    if (stream) {
      push('\nstream\n');
      push(stream);
      push('\nendstream');
    }
    push('\nendobj\n');
  };

  const w = round(page.width);
  const h = round(page.height);
  const draw = enc.encode(`q ${w} 0 0 ${h} 0 0 cm /Im0 Do Q`);

  push('%PDF-1.4\n%âãÏÓ\n');
  object(1, '<< /Type /Catalog /Pages 2 0 R >>');
  object(2, '<< /Type /Pages /Kids [3 0 R] /Count 1 >>');
  object(3, `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${w} ${h}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>`);
  object(
    4,
    `<< /Type /XObject /Subtype /Image /Width ${pixels.width} /Height ${pixels.height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>`,
    jpeg,
  );
  object(5, `<< /Length ${draw.length} >>`, draw);

  const xref = length;
  push(`xref\n0 6\n0000000000 65535 f \n`);
  for (let i = 1; i <= 5; i++) push(`${String(offsets[i]).padStart(10, '0')} 00000 n \n`);
  push(`trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);

  const out = new Uint8Array(length);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

const round = (n: number) => Math.round(n * 100) / 100;

/** The bytes of a `data:` URL. */
export function dataUrlBytes(dataUrl: string): Uint8Array {
  const base64 = dataUrl.slice(dataUrl.indexOf(',') + 1);
  const binary = atob(base64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
