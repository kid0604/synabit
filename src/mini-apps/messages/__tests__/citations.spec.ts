import { describe, it, expect } from 'vitest';
import { linkCitations } from '../citations';
import type { SourceRef } from '../types';

const sources: SourceRef[] = [
  { id: 'Notes/Pricing decision.md', title: 'Pricing decision', node_type: 'note' },
  { id: 'Notes/Pricing pushback.md', title: 'Pricing "pushback" <draft>', node_type: 'note' },
];
const label = (n: number, title: string) => `Source ${n}: ${title}`;

describe('citations in an answer', () => {
  it('turns [n] into a superscript button named for its source', () => {
    const html = linkCitations('<p>We chose per-seat [1].</p>', sources, label);
    expect(html).toBe(
      '<p>We chose per-seat <sup class="cite-mark"><button type="button" class="cite" data-cite="1"' +
        ' aria-label="Source 1: Pricing decision" title="Pricing decision">1</button></sup>.</p>',
    );
  });

  it('opens every source in a list, and each of two marks side by side', () => {
    const html = linkCitations('<p>Mai disagreed [2, 1] and [1][2].</p>', sources, label);
    expect(html.match(/data-cite="/g)).toHaveLength(4);
  });

  it('leaves a number with no source as the model wrote it', () => {
    const html = '<p>Maybe [3]. And [1, 3].</p>';
    expect(linkCitations(html, sources, label)).toBe(html);
  });

  it('leaves the text alone while the answer has no sources yet', () => {
    const html = '<p>Streaming [1]</p>';
    expect(linkCitations(html, undefined, label)).toBe(html);
    expect(linkCitations(html, [], label)).toBe(html);
  });

  it('does not touch code, links, or what only looks like a citation', () => {
    const html =
      '<p><code>v[1]</code> and <a href="#">[2]</a></p><pre><code>a[2]\n</code></pre>' +
      '<p>[[1]] and [2](x) and [2026]</p>';
    expect(linkCitations(html, sources, label)).toBe(html);
  });

  it('escapes a title before it goes into an attribute', () => {
    const html = linkCitations('<p>See [2].</p>', sources, label);
    expect(html).toContain('aria-label="Source 2: Pricing &quot;pushback&quot; &lt;draft&gt;"');
    expect(html).not.toContain('<draft>');
  });

  it('carries on after code closes', () => {
    const html = linkCitations('<p><code>x[1]</code> but here [1].</p>', sources, label);
    expect(html.match(/data-cite="1"/g)).toHaveLength(1);
    expect(html).toContain('<code>x[1]</code>');
  });
});
