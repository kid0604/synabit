import { useNodeService } from '../../../composables/useNodeService';
import { useEventBus } from '../../../composables/useEventBus';
import type { CachedArticle } from '../types/feed.types';

export function useFeedActions() {
  const ns = useNodeService();
  const bus = useEventBus();

  function sanitizeFilename(name: string): string {
    return name.replace(/[<>:"/\\|?*]/g, '-').substring(0, 80).trim() || 'Untitled';
  }

  async function clipToNote(article: CachedArticle): Promise<void> {
    const title = sanitizeFilename(article.title);
    const relPath = `Notes/${title}.json`;
    const content = `# ${article.title}\n\n**Source:** [${article.url}](${article.url})\n**Author:** ${article.author || 'Unknown'}\n**Published:** ${article.publishedAt}\n\n---\n\n${article.content || article.summary}`;

    await ns.writeNode({
      relPath,
      nodeType: 'note',
      title: article.title,
      properties: {
        tags: ['clipped', 'feed'],
        source_url: article.url,
      },
      content,
      eventType: 'created',
    });

    bus.emit('navigate:to-item', { app: 'note', itemId: relPath });
  }

  async function quickCapture(article: CachedArticle): Promise<void> {
    const id = `qc-feed-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
    const relPath = `QuickCaps/${id}.json`;
    const content = `${article.title}\n${article.url}`;

    await ns.writeNode({
      relPath,
      nodeType: 'quickcap',
      title: article.title,
      properties: {
        tags: ['feed'],
        color: 'yellow',
        source_url: article.url,
      },
      content,
      eventType: 'created',
    });

    bus.emit('navigate:to-item', { app: 'quickcap', itemId: relPath });
  }

  async function createTask(article: CachedArticle): Promise<void> {
    const id = `task-feed-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
    const relPath = `Tasks/${id}.json`;

    await ns.writeNode({
      relPath,
      nodeType: 'task',
      title: `Read: ${article.title}`,
      properties: {
        status: 'todo',
        tags: ['reading', 'feed'],
        note: article.url,
        source_url: article.url,
      },
      content: `Read and review: [${article.title}](${article.url})`,
      eventType: 'created',
    });

    bus.emit('navigate:to-item', { app: 'task', itemId: relPath });
  }

  return { clipToNote, quickCapture, createTask };
}
