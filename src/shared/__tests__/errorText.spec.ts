import { describe, it, expect } from 'vitest';
import { errorText } from '../errorText';

describe('errorText', () => {
  it('reads the message out of an AppError', () => {
    expect(errorText({ code: 'GENERAL', message: 'Syn is switched off' })).toBe('Syn is switched off');
  });

  it('keeps a plain string, and says something for anything else', () => {
    expect(errorText('boom')).toBe('boom');
    expect(errorText(new Error('broken'))).toBe('broken');
    expect(errorText(42)).toBe('42');
  });
});
