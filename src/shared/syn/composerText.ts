/**
 * What is typed into a composer, made ready to send — and no more than that.
 *
 * Both composers used to "clean" the text line by line: every line trimmed,
 * runs of spaces collapsed to one, blank lines dropped, and a line identical to
 * the one above it removed. That was aimed at a Vietnamese IME that sometimes
 * committed a line twice, and it hit everything else on the way. A pasted YAML
 * block arrived flat, a nested list lost its nesting, code lost its
 * indentation, and two deliberately identical lines became one. What Syn was
 * asked about was not what the person had written.
 *
 * So this now only removes what nobody can see or meant to send:
 *
 * * zero-width characters, which IMEs and some web pages leave behind and
 *   which make two identical-looking words different to a search;
 * * Windows line endings, so a line break is one character whatever pasted it;
 * * blank lines before the first line of text, and all whitespace after the
 *   last — an empty line left under the question by Shift+Enter.
 *
 * The first line keeps its own indentation. A snippet pasted from the middle
 * of a file starts indented, and flattening only its first line would leave it
 * misaligned with every line below.
 */
export const tidyComposerText = (text: string): string =>
  text
    .replace(/\r\n?/g, '\n')
    .replace(/[​-‍﻿]/g, '')
    .replace(/^(?:[ \t]*\n)+/, '')
    .trimEnd();
