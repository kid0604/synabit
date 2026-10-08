import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { stampElement } from '../boardFile';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';
import { CARD_SIZE } from '../vaultCards';
import { STICKY_SIZE } from '../sticky';
import { errorText } from '../../../shared/errorText';

export type AssistAction = 'summarize' | 'expand' | 'cluster' | 'tasks' | 'sketch' | 'generate';

/** A diagram asked for in words: what to draw, and what to draw it from. */
export interface GenerateRequest {
  request: string;
  /** The selection's words go along as material. */
  useSelection: boolean;
  /** A note's text, as material. */
  source?: string;
}

/** The box around some laid-out items, from the sizes they carry. */
export function extentOf(nodes: WBNode[]): { w: number; h: number } {
  if (!nodes.length) return { w: 0, h: 0 };
  const x0 = Math.min(...nodes.map((n) => n.position.x));
  const y0 = Math.min(...nodes.map((n) => n.position.y));
  const x1 = Math.max(...nodes.map((n) => n.position.x + (Number(n.data?.width) || 160)));
  const y1 = Math.max(...nodes.map((n) => n.position.y + (Number(n.data?.height) || 80)));
  return { w: x1 - x0, h: y1 - y0 };
}

/** The words an item carries, as Syn should read them. */
export function wordsOf(n: WBNode): string {
  const raw = String(n.data?.label ?? n.data?.title ?? n.data?.noteTitle ?? '');
  // Markdown marks are layout, not meaning.
  return raw.replace(/^#+\s*/gm, '').replace(/[*_`>]/g, '').replace(/\s+/g, ' ').trim();
}

interface Box { x: number; y: number; w: number; h: number }

/**
 * Syn on the canvas: one thing done to what is selected, the answer placed
 * on the board beside it. The model's side is `syn_board_assist` in Rust;
 * this side chooses what to send and where the answer goes.
 */
export function useBoardAssist(ctx: {
  store: any;
  vaultPath: () => string;
  locale: () => string;
  selectedIds: () => string[];
  /** A box for an item, as drawn. */
  boxOf: (n: WBNode) => Box;
  /** Paste items as one step, selecting them; `at` is their top-left. */
  paste: (clip: { nodes: WBNode[]; edges: WBEdge[] }, at: { x: number; y: number }) => void;
  /** Redraw the canvas and save. */
  commit: (select?: string[]) => void;
  /** A picture of the selection, base64 PNG, for a sketch. */
  pictureOf: (ids: string[]) => Promise<string | null>;
  addMindmapChildren: (parentId: string, labels: string[]) => void;
  createTasks: (tasks: { title: string; due_date?: string }[]) => Promise<{ id: string; title: string }[]>;
  notify: (message: string, kind: 'info' | 'error') => void;
  /** The middle of what is on screen, in board coordinates. */
  viewCentre: () => { x: number; y: number };
  t: (key: string, values?: Record<string, unknown>) => string;
}) {
  const busy = ref<AssistAction | null>(null);
  /** Counts the requests; an answer to one that was stopped is not used. */
  let asked = 0;
  /** Which board is open: compared by id, since taking in a sync replaces the board's object. */
  const boardKey = () => ctx.store.currentBoardId?.value ?? board();
  const board = (): WhiteboardData | null => ctx.store.currentBoardData.value;

  /** The selection's bounding box, and the place just to its right. */
  function around(nodes: WBNode[]): Box {
    const boxes = nodes.map(ctx.boxOf);
    const x = Math.min(...boxes.map((b) => b.x));
    const y = Math.min(...boxes.map((b) => b.y));
    return {
      x, y,
      w: Math.max(...boxes.map((b) => b.x + b.w)) - x,
      h: Math.max(...boxes.map((b) => b.y + b.h)) - y,
    };
  }

  /**
   * The nearest place right of `area` where a block of `w` × `h` overlaps
   * nothing on the board — so an answer lands beside what it answers, not
   * on top of the item next door.
   */
  function freeSpot(area: Box, w: number, h: number, moving?: Set<string>): { x: number; y: number } {
    // What is about to move (items being grouped) is not in the way: their
    // new frames may go where they are now.
    const others = (board()?.nodes ?? []).filter((n) => n.type !== 'frame' && !moving?.has(n.id)).map(ctx.boxOf);
    const clear = (x: number, y: number) =>
      others.every((o) => x + w + 20 <= o.x || o.x + o.w + 20 <= x || y + h + 20 <= o.y || o.y + o.h + 20 <= y);
    for (let col = 0; col < 12; col++) {
      const x = (moving ? area.x : area.x + area.w + 60) + col * 120;
      for (let row = 0; row < 20; row++) {
        const y = area.y + row * 40;
        if (clear(x, y)) return { x, y };
      }
    }
    return { x: area.x + area.w + 60, y: area.y };
  }

  async function run(action: AssistAction, generate?: GenerateRequest) {
    if (busy.value) return;
    const b = board();
    if (!b) return;
    const ids = new Set(action === 'generate' && !generate?.useSelection ? [] : ctx.selectedIds());
    const chosen = b.nodes.filter((n) => ids.has(n.id));
    if (!chosen.length && action !== 'generate') return;
    const items = chosen
      .filter((n) => n.type !== 'stroke' && n.type !== 'frame' && n.type !== 'image')
      .map((n) => ({ id: n.id, kind: n.type, text: wordsOf(n) }))
      .filter((i) => i.text);

    let image: string | null = null;
    if (action === 'sketch') {
      image = await ctx.pictureOf([...ids]);
      if (!image) return;
    } else if (action === 'generate') {
      if (!generate?.request.trim()) return;
    } else if (!items.length) {
      ctx.notify(ctx.t('whiteboard.syn.nothing_to_read'), 'error');
      return;
    }

    busy.value = action;
    const mine = ++asked;
    const key = boardKey();
    ctx.notify(ctx.t('whiteboard.syn.working'), 'info');
    try {
      const answer = await invoke<any>('syn_board_assist', {
        vaultPath: ctx.vaultPath(),
        action,
        items,
        image,
        locale: ctx.locale(),
        request: action === 'generate' ? generate!.request.trim() : undefined,
        source: action === 'generate' ? generate!.source : undefined,
      });
      // Stopped while it was being worked on: nothing is put on the board.
      if (mine !== asked) return;
      // The answer is about the board it was asked on. Another one open by
      // now is not where it goes — and tasks are not made for it either. The
      // same board, written by a sync or by Syn in the chat meanwhile, is
      // still the same board.
      if (boardKey() !== key || !board()) {
        ctx.notify(ctx.t('whiteboard.syn.board_changed'), 'info');
        return;
      }
      // Beside what it was drawn from; with nothing selected, where the person is looking.
      const centre = ctx.viewCentre();
      const area = chosen.length ? around(chosen) : { x: centre.x - 300, y: centre.y - 200, w: 0, h: 0 };
      if (action === 'summarize') placeSummary(answer.summary, freeSpot(area, 260, 260));
      else if (action === 'expand') {
        const cols = Math.min(3, answer.ideas.length);
        const rows = Math.ceil(answer.ideas.length / 3);
        placeIdeas(answer.ideas, chosen, freeSpot(area, cols * (STICKY_SIZE + 20), rows * (STICKY_SIZE + 20)));
      } else if (action === 'cluster') arrangeGroups(answer.groups, area);
      else if (action === 'tasks') {
        const count = Math.max(1, answer.tasks.length);
        await placeTasks(answer.tasks, freeSpot(area, CARD_SIZE.width, count * (CARD_SIZE.height + 12)));
      }
      else if (action === 'sketch') {
        // In clear space near the sketch, not over whatever is under it.
        const { w, h } = extentOf(answer.nodes);
        ctx.paste({ nodes: answer.nodes, edges: autoSides(answer.edges) }, freeSpot({ ...area, y: area.y + area.h + 60, h: 0 }, w, h));
      }
      else if (action === 'generate') placeDiagram(answer, area);
    } catch (err) {
      if (mine !== asked) return;
      ctx.notify(failure(errorText(err)), 'error');
    } finally {
      if (mine === asked) busy.value = null;
    }
  }

  /**
   * What went wrong, in the person's language. The app's own failures come
   * with a code (`[syn:no_ideas] …`) and are said in words of this app;
   * anything else — a provider's own message — is shown as it came.
   */
  function failure(text: string): string {
    if (/switched off/i.test(text)) return ctx.t('whiteboard.syn.off');
    const code = /\[syn:([a-z_]+)\]/.exec(text)?.[1];
    if (code === 'no_model') return ctx.t('whiteboard.syn.no_model');
    const said = text.replace(/^Error:\s*/, '').replace(/\[syn:[a-z_]+\]\s*/, '');
    if (!code) return said;
    const key = `whiteboard.syn.err.${code}`;
    const words = ctx.t(key, { detail: said.replace(/^Syn could not do that:\s*/, '') });
    return words === key ? said : words;
  }

  /**
   * Stop waiting for Syn. The request may still finish on the provider's side;
   * its answer is not used, and the board's Syn actions are free again.
   */
  function cancel() {
    if (!busy.value) return;
    asked++;
    busy.value = null;
    ctx.notify(ctx.t('whiteboard.syn.cancelled'), 'info');
  }

  /**
   * Lines Syn draws take the sides that face each other, so they stay neat as
   * the person drags the boxes about (routing.ts).
   */
  function autoSides(edges: WBEdge[]): WBEdge[] {
    return edges.map((e) => ({ ...e, data: { ...(e.data ?? {}), sides: 'auto' } }));
  }

  /** A diagram from a description, in the nearest clear space, and said so. */
  function placeDiagram(answer: { title?: string; nodes: WBNode[]; edges: WBEdge[] }, area: Box) {
    if (!answer.nodes?.length) return;
    const { w, h } = extentOf(answer.nodes);
    const at = freeSpot(area, w, h);
    ctx.paste({ nodes: answer.nodes, edges: autoSides(answer.edges ?? []) }, at);
    ctx.notify(answer.title ? ctx.t('whiteboard.syn.drawn_titled', { title: answer.title }) : ctx.t('whiteboard.syn.drawn'), 'info');
  }

  function sticky(label: string, at: { x: number; y: number }, color: string, size = STICKY_SIZE): WBNode {
    return { id: ctx.store.generateId('sticky'), type: 'sticky', position: at, data: { label, color, width: size, height: size } };
  }

  function placeSummary(summary: string, at: { x: number; y: number }) {
    ctx.paste({ nodes: [sticky(summary, at, 'blue', 260)], edges: [] }, at);
  }

  /** New ideas: branches of a mind-map item, or a row of sticky notes. */
  function placeIdeas(ideas: string[], chosen: WBNode[], at: { x: number; y: number }) {
    if (chosen.length === 1 && chosen[0].type === 'mindmap') {
      ctx.addMindmapChildren(chosen[0].id, ideas);
      return;
    }
    const nodes = ideas.map((idea, i) => sticky(idea, { x: at.x + (i % 3) * (STICKY_SIZE + 20), y: at.y + Math.floor(i / 3) * (STICKY_SIZE + 20) }, 'yellow'));
    ctx.paste({ nodes, edges: [] }, at);
  }

  /**
   * Groups: a frame for each, side by side where the selection was, its items
   * moved into it in a grid. The items themselves are moved, not copied.
   */
  function arrangeGroups(groups: { name: string; items: string[] }[], area: Box) {
    const b = board();
    if (!b) return;
    ctx.store.pushUndoState();
    const byId = new Map(b.nodes.map((n) => [n.id, n]));
    const pad = 24;
    // Each group's size first, so the row of frames can be put where it fits:
    // where the items were, unless something else is in the way there.
    const laid = groups
      .map((group) => {
        const members = group.items.map((id) => byId.get(id)).filter((n): n is WBNode => !!n);
        if (!members.length) return null;
        const cell = Math.max(...members.map((n) => Math.max(ctx.boxOf(n).w, ctx.boxOf(n).h))) + 16;
        const cols = Math.min(2, members.length);
        const rows = Math.ceil(members.length / cols);
        return { group, members, cell, cols, rows, w: pad * 2 + cols * cell - 16, h: pad * 2 + rows * cell - 16 };
      })
      .filter((g): g is NonNullable<typeof g> => !!g);
    const total = laid.reduce((sum, g) => sum + g.w + 40, -40);
    const tallest = Math.max(0, ...laid.map((g) => g.h));
    const at = freeSpot(area, total, tallest, new Set(laid.flatMap((g) => g.members.map((m) => m.id))));
    let x = at.x;
    const made: string[] = [];
    for (const { group, members, cell, cols, rows } of laid) {
      const frame: WBNode = {
        id: ctx.store.generateId('frame'),
        type: 'frame',
        position: { x, y: at.y },
        data: { label: group.name, width: pad * 2 + cols * cell - 16, height: pad * 2 + rows * cell - 16 },
      };
      stampElement(frame);
      b.nodes.push(frame);
      made.push(frame.id);
      members.forEach((n, i) => {
        n.position = { x: x + pad + (i % cols) * cell, y: at.y + pad + Math.floor(i / cols) * cell };
        stampElement(n);
      });
      x += frame.data.width + 40;
    }
    ctx.commit(made);
  }

  /** Tasks, made in the vault and placed on the board as their cards. */
  async function placeTasks(tasks: { title: string; due_date?: string }[], at: { x: number; y: number }) {
    const made = await ctx.createTasks(tasks);
    const nodes: WBNode[] = made.map((task, i) => ({
      id: ctx.store.generateId('card'),
      type: 'card',
      position: { x: at.x, y: at.y + i * (CARD_SIZE.height + 12) },
      data: { ref: task.id, kind: 'task', title: task.title, ...CARD_SIZE },
    }));
    if (nodes.length) ctx.paste({ nodes, edges: [] }, at);
    ctx.notify(ctx.t('whiteboard.syn.tasks_made', { count: made.length }), 'info');
  }

  return { busy, run, cancel };
}
