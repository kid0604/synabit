import { describe, it, expect } from 'vitest';
import { said } from '../said';

describe('What the backend said', () => {
  /// The shape `AppError` serialises to. Every refusal the query engine
  /// writes arrived as "[object Object]" until this existed.
  it('reads the shape a Tauri command rejects with', () => {
    expect(said({ code: 'GENERAL_ERROR', message: "'hôm-nào-đó' is not a time." }))
      .toBe("'hôm-nào-đó' is not a time.");
  });

  it('reads an ordinary Error too, without its class name', () => {
    expect(said(new Error('nothing to match on'))).toBe('nothing to match on');
  });

  it('passes a string through', () => {
    expect(said('plain')).toBe('plain');
  });

  /// Better the raw shape than an empty box: an empty box reads as "nothing
  /// went wrong".
  it('never comes back empty when something did go wrong', () => {
    expect(said({ code: 'X', message: '   ' })).toContain('X');
    expect(said(null)).toBeTruthy();
    expect(said(undefined)).toBeTruthy();
    expect(said({})).toBeTruthy();
  });
});
