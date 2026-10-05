import { describe, expect, it } from 'vitest';
import { slidesOf } from '../presentation';
import { sharePage, svgFromDataUrl } from '../sharePage';
import type { WBNode } from '../boardFile';

const frame = (id: string, x: number, y: number, label = id): WBNode => ({ id, type: 'frame', position: { x, y }, data: { label, width: 400, height: 300 } });

describe('the order frames are presented in', () => {
  it('reads row by row, left to right, with rows that are not quite level', () => {
    const order = slidesOf([frame('c', 0, 500), frame('b', 500, 20), frame('a', 0, 0), frame('d', 520, 480)]).map((s) => s.id);
    expect(order).toEqual(['a', 'b', 'c', 'd']);
  });

  it('presents frames only, with their names and boxes', () => {
    const slides = slidesOf([frame('a', 10, 20, 'Intro'), { id: 's', type: 'sticky', position: { x: 0, y: 0 }, data: {} }]);
    expect(slides).toEqual([{ id: 'a', title: 'Intro', box: { x: 10, y: 20, width: 400, height: 300 } }]);
  });
});

describe('a board shared as a web page', () => {
  const labels = { prev: 'Prev', next: 'Next', fit: 'Fit', zoomIn: 'In', zoomOut: 'Out', hint: 'Drag' };
  const page = (title: string, svg: string) => sharePage({ title, svg, width: 100, height: 50, frames: [], lang: 'en', labels });

  it('cannot be broken out of by what is written on the board', () => {
    const html = page('<b>Plan</b>', '<svg><text>"</script><script>alert(1)</script>"</text></svg>');
    expect(html).toContain('<title>&lt;b&gt;Plan&lt;/b&gt;</title>');
    // One script, and the board's words inside it only as escaped data.
    expect(html.match(/<script>/g)).toHaveLength(1);
    expect(html.match(/<\/script>/g)).toHaveLength(1);
    const data = html.slice(html.indexOf('var D = ') + 8, html.indexOf(';\n  var stage'));
    expect(JSON.parse(data).svg).toContain('</script><script>alert(1)</script>');
  });

  it('asks for nothing from the network', () => {
    expect(page('B', '<svg/>')).toContain("default-src 'none'");
  });

  it('reads the picture back out of the export', () => {
    expect(svgFromDataUrl(`data:image/svg+xml;charset=utf-8,${encodeURIComponent('<svg a="1"/>')}`)).toBe('<svg a="1"/>');
    expect(svgFromDataUrl(`data:image/svg+xml;base64,${btoa('<svg/>')}`)).toBe('<svg/>');
  });
});
