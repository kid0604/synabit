import { describe, expect, it } from 'vitest';
import { forCanvas, PANE_NODE_TYPES } from '../components/BoardPane.vue';

const item = (type: string, data: any = {}) => ({ id: 'n', type, position: { x: 1, y: 2 }, data });

describe('the board pane beside an answer', () => {
  it('draws a frame Syn wrote as a frame, at the app\'s default size, below what it holds', () => {
    const frame = forCanvas(item('frame', { label: 'Data centre' }));
    const box = forCanvas(item('shape', { label: 'API' }));
    expect(frame.type).toBe('frame');
    expect(frame.style).toEqual({ width: '480px', height: '320px' });
    expect(frame.zIndex).toBeLessThan(box.zIndex);
  });

  it('locks every item so nothing in it edits the vault, without touching the file\'s data', () => {
    const data = { label: 'Call the bank', editing: true };
    for (const type of PANE_NODE_TYPES) {
      const n = forCanvas(item(type, data));
      expect(n.data.locked).toBe(true);
      expect(n.data.editing).toBeUndefined();
    }
    expect(data).toEqual({ label: 'Call the bank', editing: true });
  });

  it('draws a kind it does not know as a plain labelled box', () => {
    const n = forCanvas(item('hologram', { title: 'Mystery' }));
    expect(n.type).toBe('default');
    expect(n.label).toBe('Mystery');
    expect(n.style).toEqual({ width: '160px', height: '80px' });
  });

  it('lets text and ink size themselves', () => {
    expect(forCanvas(item('text')).style).toBeUndefined();
    expect(forCanvas(item('stroke')).style).toBeUndefined();
  });
});
