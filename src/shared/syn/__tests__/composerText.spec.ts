import { describe, it, expect } from 'vitest';

import { tidyComposerText } from '../composerText';
import askBar from '../AskBar.vue?raw';
import chatPanel from '../../../mini-apps/messages/components/ChatPanel.vue?raw';
import messagesApp from '../../../mini-apps/messages/MessagesApp.vue?raw';

/**
 * What leaves the composer is what was written.
 *
 * Every line used to be trimmed and squeezed, blank lines dropped and repeated
 * lines removed — which flattened pasted YAML, code and nested lists before Syn
 * ever saw them. See `composerText.ts`.
 */
describe('tidying what was typed', () => {
  it('keeps indentation inside the text', () => {
    const yaml = 'server:\n  port: 8080\n  hosts:\n    - a\n    - b';
    expect(tidyComposerText(yaml)).toBe(yaml);
  });

  it('keeps runs of spaces and blank lines between paragraphs', () => {
    const text = 'first  paragraph\n\nsecond\tparagraph';
    expect(tidyComposerText(text)).toBe(text);
  });

  it('keeps a line that repeats the one above it', () => {
    expect(tidyComposerText('yes\nyes')).toBe('yes\nyes');
  });

  it('keeps the first line of an indented snippet indented', () => {
    expect(tidyComposerText('\n\n    return x;\n}')).toBe('    return x;\n}');
  });

  it('drops blank lines around the text and whitespace after it', () => {
    expect(tidyComposerText('  \n\nhello\n\n  \n')).toBe('hello');
  });

  it('removes zero-width characters and Windows line endings', () => {
    expect(tidyComposerText('a​b\r\nc﻿')).toBe('ab\nc');
  });

  it('is empty for nothing but whitespace', () => {
    expect(tidyComposerText(' \n\t\n​ ')).toBe('');
  });

  /**
   * The rule is in one place. Each of these composers had its own copy of the
   * old one, and fixing two of three would have left the third quietly
   * flattening code.
   */
  it('is the rule every composer uses', () => {
    for (const [name, source] of [
      ['AskBar', askBar],
      ['ChatPanel', chatPanel],
      ['MessagesApp', messagesApp],
    ] as const) {
      expect(source, `${name} should tidy with tidyComposerText`).toContain('tidyComposerText(');
      expect(source, `${name} still squeezes lines`).not.toContain("replace(/\\s+/g, ' ')");
    }
  });
});
