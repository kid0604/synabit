/**
 * Boards to start from.
 *
 * A blank board is a fine start for a sketch and a slow one for a meeting
 * that always has the same shape: a weekly review, a kickoff, a retro. These
 * are those shapes, laid out with frames and sticky notes the way a person
 * would lay them out by hand, with the words in the user's language.
 *
 * The list follows what other whiteboards offer to start from (Miro, FigJam,
 * Whimsical, Lucid, XMind), in the groups they share: meetings and workshops,
 * strategy and planning, diagrams, mind maps, product and design, and one's
 * own week and studies. Each is drawn with what this board has — frames,
 * sticky notes, shapes from its library, mind maps — so all of it can be
 * worked on once it is there, not only looked at.
 *
 * Each builds items around (0, 0); the board places them where they go and
 * gives them ids of its own.
 */
import type { WBEdge, WBNode } from './boardFile';
import { today, weekStart } from './vaultCards';
import { buildMindmap, estimateSize, MINDMAP_COLORS, setLayout, tidyTree, type MapLayout, type OutlineItem } from './mindmap';
import { buildStroke } from './composables/useFreeDrawing';

type T = (key: string) => string;

/** The groups the picker shows, in its order. */
export const TEMPLATE_CATEGORIES = ['meetings', 'strategy', 'diagrams', 'mindmaps', 'product', 'personal'] as const;
export type TemplateCategory = (typeof TEMPLATE_CATEGORIES)[number];

export interface BoardTemplate {
  id: string;
  category: TemplateCategory;
  /** i18n keys under `whiteboard.templates.<id>`: `name`, `desc`, and the words on the board. */
  build: (t: T) => { nodes: WBNode[]; edges: WBEdge[] };
}

// ── Parts ───────────────────────────────────────────────────

let seq = 0;
const id = (kind: string) => `tpl-${kind}-${++seq}`;

const frame = (x: number, y: number, width: number, height: number, label: string, color?: string): WBNode => ({
  id: id('frame'), type: 'frame', position: { x, y }, data: { label, width, height, ...(color ? { color } : {}) },
});
const lane = (x: number, y: number, width: number, height: number, label: string, color?: string): WBNode => {
  const f = frame(x, y, width, height, label, color);
  f.data.variant = 'lane';
  return f;
};
const sticky = (x: number, y: number, label: string, color = 'yellow', size = 180): WBNode => ({
  id: id('sticky'), type: 'sticky', position: { x, y }, data: { label, color, width: size, height: size },
});
const text = (x: number, y: number, label: string, fontSize = 16, width = 320): WBNode => ({
  id: id('text'), type: 'text', position: { x, y }, data: { label, fontSize, width },
});
const box = (
  x: number, y: number, label: string, shapeType = 'roundedRect', color = '#3b82f6', width = 160, height = 72, extra: Record<string, unknown> = {},
): WBNode => ({
  id: id('shape'), type: 'shape', position: { x, y }, data: { shapeType, label, color, width, height, ...extra },
});
/** A box with a pale fill of its own colour. */
const filled = (x: number, y: number, label: string, shapeType: string, color: string, width = 160, height = 72): WBNode =>
  box(x, y, label, shapeType, color, width, height, { fillColor: `${color}22` });
const title = (t: T, prefix: string, y = -90): WBNode => text(0, y, `# ${t(`${prefix}.name`)}`, 18, 600);

/** The sides two boxes face each other by, as the board would pick them. */
function facing(a: WBNode, b: WBNode): [string, string] {
  const size = (n: WBNode) => ({ w: Number(n.data.width) || 160, h: Number(n.data.height) || 72 });
  const [sa, sb] = [size(a), size(b)];
  const dx = b.position.x + sb.w / 2 - (a.position.x + sa.w / 2);
  const dy = b.position.y + sb.h / 2 - (a.position.y + sa.h / 2);
  if (Math.abs(dy) > Math.abs(dx)) return dy > 0 ? ['bottom', 'top'] : ['top', 'bottom'];
  return dx > 0 ? ['right', 'left'] : ['left', 'right'];
}

/**
 * A line between two items, its sides following them as they move. Stepped
 * by default, as a diagram is drawn and as the board routes around boxes.
 */
const link = (
  source: WBNode, target: WBNode,
  opts: { label?: string; curve?: boolean; arrow?: boolean; dashed?: boolean; sides?: [string, string] } = {},
): WBEdge => {
  // Fixed sides, for a line back that would otherwise lie on the line there.
  const [from, to] = opts.sides ?? facing(source, target);
  return {
    id: id('edge'), source: source.id, target: target.id, type: opts.curve ? 'default' : 'step',
    sourceHandle: from, targetHandle: to,
    data: {
      ...(opts.sides ? {} : { sides: 'auto' }),
      ...(opts.arrow === false ? {} : { markerEnd: 'arrow' }),
      ...(opts.label ? { label: opts.label } : {}),
      ...(opts.dashed ? { dashStyle: 'dashed' } : {}),
    },
  };
};
/** The old left-to-right line of the first templates, kept for theirs. */
const line = (source: WBNode, target: WBNode, label?: string): WBEdge => ({
  id: id('edge'), source: source.id, target: target.id, type: 'default',
  sourceHandle: 'right', targetHandle: 'left',
  data: { markerEnd: 'arrow', ...(label ? { label } : {}) },
});

/** A straight stroke of ink from one point to another: a fish's bones, a timeline's axis. */
function rule(points: [number, number][], color = '#64748b', size = 4): WBNode {
  const s = buildStroke(points.map(([x, y]) => [x, y, 0.5]), size)!;
  return {
    id: id('stroke'), type: 'stroke', position: { x: s.x, y: s.y },
    data: { svgPath: s.svgPath, points: s.points, width: s.width, height: s.height, color, size, opacity: 0.9 },
  };
}

/** A row of frames, each with one sticky note inviting the first entry. */
function columns(t: T, prefix: string, keys: string[], colors: string[], width = 320, height = 520, y = 0) {
  const nodes: WBNode[] = [];
  keys.forEach((key, i) => {
    const x = i * (width + 40);
    nodes.push(frame(x, y, width, height, t(`${prefix}.${key}`)));
    nodes.push(sticky(x + (width - 180) / 2, y + 40, t(`${prefix}.${key}_hint`), colors[i % colors.length]));
  });
  return nodes;
}

/** Four frames two by two, each with a sticky note saying what goes in it. */
function quadrants(t: T, prefix: string, cells: [key: string, color: string][], width = 420, height = 320) {
  return cells.flatMap(([key, color], i) => {
    const x = (i % 2) * (width + 20);
    const y = Math.floor(i / 2) * (height + 20);
    return [frame(x, y, width, height, t(`${prefix}.${key}`)), sticky(x + 40, y + 50, t(`${prefix}.${key}_hint`), color, 170)];
  });
}

/** A frame with a line of words in it saying what it is for. */
function block(t: T, prefix: string, key: string, x: number, y: number, width: number, height: number): WBNode[] {
  return [frame(x, y, width, height, t(`${prefix}.${key}`)), text(x + 16, y + 44, t(`${prefix}.${key}_hint`), 13, width - 32)];
}

/** A mind map from a tree of words, laid out as a map, a logic chart or an org chart. */
function mindmapOf(tree: OutlineItem, layout: MapLayout = 'map') {
  const built = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, id);
  if (layout !== 'map') {
    setLayout(built.rootId, layout, built.nodes, built.edges);
    const placed = tidyTree(built.rootId, built.nodes, built.edges, estimateSize);
    for (const n of built.nodes) {
      const p = placed.get(n.id);
      if (p) n.position = p;
    }
  }
  return { nodes: built.nodes, edges: built.edges };
}

/** One idea and the ideas under it, by key. */
const idea = (k: (s: string) => string, key: string, children: OutlineItem[] = [], extra: Partial<OutlineItem> = {}): OutlineItem =>
  ({ label: k(key), children, ...extra });

/** The words of one template, by their key under it. */
const words = (t: T, tpl: string) => (key: string) => t(`whiteboard.templates.${tpl}.${key}`);

// ── The templates ───────────────────────────────────────────

export const TEMPLATES: BoardTemplate[] = [
  // Meetings and workshops ──────────────────────────────────
  {
    id: 'project_kickoff',
    category: 'meetings',
    build: (t) => {
      const k = words(t, 'project_kickoff');
      const nodes = [
        text(0, -90, `# ${k('name')}`, 18, 600),
        frame(0, 0, 460, 260, k('goal')),
        sticky(40, 50, k('goal_hint'), 'yellow', 180),
        frame(500, 0, 460, 260, k('scope')),
        sticky(540, 50, k('in_scope'), 'green', 170),
        sticky(750, 50, k('out_scope'), 'pink', 170),
        frame(1000, 0, 360, 260, k('people')),
        sticky(1090, 50, k('people_hint'), 'blue', 180),
      ];
      // Milestones on a line, left to right.
      const m = ['m1', 'm2', 'm3', 'm4'].map((key, i) => box(40 + i * 330, 380, k(key), 'pill', '#7c3aed', 200, 56));
      nodes.push(frame(0, 300, 1360, 180, k('milestones')), ...m);
      nodes.push(frame(0, 520, 660, 260, k('risks')), sticky(40, 570, k('risks_hint'), 'orange', 180));
      nodes.push(frame(700, 520, 660, 260, k('questions')), sticky(740, 570, k('questions_hint'), 'purple', 180));
      return { nodes, edges: m.slice(1).map((n, i) => line(m[i], n)) };
    },
  },
  {
    id: 'retro',
    category: 'meetings',
    build: (t) => ({
      nodes: [
        text(0, -90, `# ${t('whiteboard.templates.retro.name')}`, 18, 600),
        ...columns(t, 'whiteboard.templates.retro', ['well', 'improve', 'actions'], ['green', 'pink', 'blue'], 360, 520),
      ],
      edges: [],
    }),
  },
  {
    id: 'brainstorm',
    category: 'meetings',
    build: (t) => {
      const k = words(t, 'brainstorm');
      const colors = ['yellow', 'pink', 'blue', 'green', 'orange', 'purple'];
      return {
        nodes: [
          title(t, 'whiteboard.templates.brainstorm'),
          frame(0, 0, 1140, 160, k('question')),
          text(40, 64, k('question_hint'), 20, 1060),
          frame(0, 200, 760, 520, k('ideas')),
          ...colors.map((c, i) => sticky(40 + (i % 3) * 240, 260 + Math.floor(i / 3) * 220, i ? '' : k('ideas_hint'), c, 180)),
          frame(800, 200, 340, 250, k('top')),
          sticky(880, 250, k('top_hint'), 'green', 180),
          frame(800, 470, 340, 250, k('parking')),
          sticky(880, 520, k('parking_hint'), 'gray', 180),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'meeting_notes',
    category: 'meetings',
    build: (t) => {
      const k = words(t, 'meeting_notes');
      return {
        nodes: [
          title(t, 'whiteboard.templates.meeting_notes', -110),
          text(0, -56, k('attendees'), 14, 900),
          frame(0, 0, 360, 560, k('agenda')),
          text(20, 48, k('agenda_items'), 14, 320),
          frame(400, 0, 460, 560, k('notes')),
          sticky(440, 50, k('notes_hint'), 'yellow', 180),
          frame(900, 0, 380, 270, k('decisions')),
          sticky(940, 50, k('decisions_hint'), 'green', 180),
          frame(900, 290, 380, 270, k('actions')),
          text(920, 338, k('actions_items'), 14, 340),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'standup',
    category: 'meetings',
    build: (t) => ({
      nodes: [
        title(t, 'whiteboard.templates.standup'),
        ...columns(t, 'whiteboard.templates.standup', ['yesterday', 'today', 'blockers'], ['green', 'blue', 'pink'], 320, 480),
      ],
      edges: [],
    }),
  },
  {
    id: 'lean_coffee',
    category: 'meetings',
    build: (t) => ({
      nodes: [
        title(t, 'whiteboard.templates.lean_coffee'),
        ...columns(t, 'whiteboard.templates.lean_coffee', ['to_discuss', 'discussing', 'discussed'], ['yellow', 'orange', 'green'], 320, 560),
      ],
      edges: [],
    }),
  },
  {
    id: 'four_ls',
    category: 'meetings',
    build: (t) => ({
      nodes: [
        title(t, 'whiteboard.templates.four_ls'),
        ...quadrants(t, 'whiteboard.templates.four_ls', [['liked', 'green'], ['learned', 'blue'], ['lacked', 'pink'], ['longed', 'purple']]),
      ],
      edges: [],
    }),
  },

  // Strategy and planning ───────────────────────────────────
  {
    id: 'swot',
    category: 'strategy',
    build: (t) => ({
      nodes: [
        title(t, 'whiteboard.templates.swot'),
        ...quadrants(t, 'whiteboard.templates.swot', [['strengths', 'green'], ['weaknesses', 'pink'], ['opportunities', 'blue'], ['threats', 'orange']]),
      ],
      edges: [],
    }),
  },
  {
    id: 'eisenhower',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'eisenhower');
      return {
        nodes: [
          title(t, 'whiteboard.templates.eisenhower', -130),
          text(0, -40, `**${k('urgent')}**`, 14, 420),
          text(440, -40, `**${k('not_urgent')}**`, 14, 420),
          ...quadrants(t, 'whiteboard.templates.eisenhower', [['do', 'pink'], ['schedule', 'blue'], ['delegate', 'yellow'], ['drop', 'gray']]),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'impact_effort',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'impact_effort');
      return {
        nodes: [
          title(t, 'whiteboard.templates.impact_effort', -130),
          text(0, -40, `**${k('low_effort')}**`, 14, 420),
          text(440, -40, `**${k('high_effort')}**`, 14, 420),
          ...quadrants(t, 'whiteboard.templates.impact_effort', [['quick_wins', 'green'], ['big_bets', 'blue'], ['fill_ins', 'yellow'], ['money_pits', 'pink']]),
          text(0, 680, k('axes'), 13, 860),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'okr',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'okr');
      const objective = filled(330, 0, k('objective'), 'roundedRect', '#7c3aed', 520, 90);
      const krs = ['kr1', 'kr2', 'kr3'].map((key, i) => filled(i * 420, 190, k(key), 'roundedRect', '#3b82f6', 340, 80));
      const work = krs.map((kr, i) => sticky(kr.position.x + 80, 340, i ? '' : k('initiative_hint'), 'yellow', 180));
      return {
        nodes: [title(t, 'whiteboard.templates.okr'), objective, ...krs, ...work, text(0, 560, k('scoring'), 13, 1180)],
        edges: [...krs.map((kr) => link(objective, kr)), ...krs.map((kr, i) => link(kr, work[i], { arrow: false }))],
      };
    },
  },
  {
    id: 'pros_cons',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'pros_cons');
      return {
        nodes: [
          title(t, 'whiteboard.templates.pros_cons'),
          frame(0, 0, 460, 440, k('pros'), '#22c55e'),
          sticky(40, 50, k('pros_hint'), 'green', 180),
          frame(500, 0, 460, 440, k('cons'), '#ec4899'),
          sticky(540, 50, k('cons_hint'), 'pink', 180),
          frame(0, 480, 960, 220, k('decision')),
          sticky(40, 520, k('decision_hint'), 'blue', 160),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'lean_canvas',
    category: 'strategy',
    build: (t) => {
      const p = 'whiteboard.templates.lean_canvas';
      return { nodes: [title(t, p), ...canvas(t, p, ['problem', 'solution', 'metrics', 'uvp', 'advantage', 'channels', 'segments', 'costs', 'revenue'])], edges: [] };
    },
  },
  {
    id: 'business_model_canvas',
    category: 'strategy',
    build: (t) => {
      const p = 'whiteboard.templates.business_model_canvas';
      return { nodes: [title(t, p), ...canvas(t, p, ['partners', 'activities', 'resources', 'value', 'relationships', 'channels', 'segments', 'costs', 'revenue'])], edges: [] };
    },
  },
  {
    id: 'fishbone',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'fishbone');
      const head = filled(1040, 210, k('problem'), 'rectangle', '#ef4444', 220, 100);
      const nodes: WBNode[] = [title(t, 'whiteboard.templates.fishbone'), rule([[0, 260], [1040, 260]], '#64748b', 6), head];
      ['people', 'process', 'tools', 'materials', 'environment', 'measures'].forEach((key, i) => {
        const x = 120 + (i % 3) * 320;
        const top = i < 3;
        nodes.push(box(x - 90, top ? 30 : 442, k(key), 'pill', '#f59e0b', 180, 48));
        nodes.push(rule([[x, top ? 78 : 442], [x + 130, 260]], '#94a3b8', 3));
      });
      // Between the first two bones, clear of both.
      nodes.push(sticky(290, 100, k('cause_hint'), 'yellow', 120));
      return { nodes, edges: [] };
    },
  },
  {
    id: 'five_whys',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'five_whys');
      const problem = filled(0, 0, k('problem'), 'roundedRect', '#ef4444', 340, 72);
      const whys = [1, 2, 3, 4, 5].map((n) => box(0, n * 120, k(`why${n}`), 'roundedRect', '#3b82f6', 340, 64));
      const root = filled(0, 720, k('root'), 'roundedRect', '#22c55e', 340, 72);
      const chain = [problem, ...whys, root];
      return {
        nodes: [title(t, 'whiteboard.templates.five_whys'), ...chain, sticky(400, 700, k('fix_hint'), 'green', 180)],
        edges: chain.slice(1).map((n, i) => link(chain[i], n, { label: i < 5 ? k('why') : undefined })),
      };
    },
  },
  {
    id: 'timeline',
    category: 'strategy',
    build: (t) => {
      const k = words(t, 'timeline');
      const nodes: WBNode[] = [title(t, 'whiteboard.templates.timeline'), rule([[0, 300], [1500, 300]], '#7c3aed', 6)];
      ['e1', 'e2', 'e3', 'e4', 'e5'].forEach((key, i) => {
        const x = 150 + i * 300;
        const up = i % 2 === 0;
        nodes.push(box(x - 12, 288, '', 'ellipse', '#7c3aed', 24, 24, { fillColor: '#7c3aed' }));
        nodes.push(rule([[x, up ? 236 : 312], [x, up ? 288 : 364]], '#c4b5fd', 3));
        nodes.push(filled(x - 100, up ? 160 : 364, k(key), 'roundedRect', '#7c3aed', 200, 76));
        nodes.push(text(x - 100, up ? 120 : 448, `**${k(`${key}_when`)}**`, 13, 200));
      });
      return { nodes, edges: [] };
    },
  },

  // Diagrams ────────────────────────────────────────────────
  {
    id: 'flowchart',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'flowchart');
      const start = filled(120, 0, k('start'), 'fcTerminator', '#22c55e', 160, 60);
      const step = box(110, 120, k('step'), 'process', '#3b82f6', 180, 70);
      const check = box(100, 250, k('check'), 'decision', '#f59e0b', 200, 110);
      const next = box(110, 420, k('next'), 'process', '#3b82f6', 180, 70);
      const end = filled(120, 550, k('end'), 'fcTerminator', '#ef4444', 160, 60);
      const fix = box(420, 270, k('fix'), 'process', '#64748b', 180, 70);
      return {
        nodes: [title(t, 'whiteboard.templates.flowchart', -80), start, step, check, next, end, fix],
        edges: [
          link(start, step), link(step, check), link(check, next, { label: k('yes') }), link(next, end),
          link(check, fix, { label: k('no') }), link(fix, step),
        ],
      };
    },
  },
  {
    id: 'swimlane',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'swimlane');
      const lanes = [['customer', '#3b82f6'], ['sales', '#10b981'], ['warehouse', '#f59e0b']].map(([key, c], i) => lane(0, i * 200, 1260, 200, k(key), c));
      const order = box(100, 64, k('order'), 'process', '#3b82f6', 170, 72);
      const check = box(360, 264, k('check'), 'process', '#10b981', 170, 72);
      const stock = box(620, 245, k('in_stock'), 'decision', '#10b981', 170, 110);
      const ship = box(880, 464, k('ship'), 'process', '#f59e0b', 170, 72);
      const tell = box(620, 64, k('back_order'), 'process', '#3b82f6', 170, 72);
      const receive = filled(1060, 64, k('receive'), 'fcTerminator', '#3b82f6', 160, 64);
      return {
        nodes: [title(t, 'whiteboard.templates.swimlane'), ...lanes, order, check, stock, ship, tell, receive],
        edges: [
          link(order, check), link(check, stock), link(stock, ship, { label: k('yes') }),
          link(stock, tell, { label: k('no') }), link(ship, receive),
        ],
      };
    },
  },
  {
    id: 'org_chart',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'org_chart');
      const head = filled(520, 0, k('lead'), 'roundedRect', '#7c3aed', 200, 70);
      const heads = ['h1', 'h2', 'h3'].map((key, i) => filled(120 + i * 400, 160, k(key), 'roundedRect', '#3b82f6', 200, 70));
      const teams = heads.flatMap((h, i) =>
        [0, 1].map((j) => box(h.position.x - 105 + j * 210, 320, k(`t${i * 2 + j + 1}`), 'roundedRect', '#10b981', 190, 64)));
      return {
        nodes: [title(t, 'whiteboard.templates.org_chart'), head, ...heads, ...teams],
        edges: [...heads.map((h) => link(head, h, { arrow: false })), ...teams.map((tm, i) => link(heads[Math.floor(i / 2)], tm, { arrow: false }))],
      };
    },
  },
  {
    id: 'system_diagram',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'system_diagram');
      const client = box(40, 170, k('client'), 'netDesktop', '#64748b', 140, 100);
      const gateway = box(300, 180, k('gateway'), 'roundedRect', '#3b82f6');
      const svcA = box(560, 90, k('service_a'), 'roundedRect', '#10b981');
      const svcB = box(560, 270, k('service_b'), 'roundedRect', '#10b981');
      const db = box(860, 80, k('database'), 'cylinder', '#f59e0b', 120, 100);
      const cache = box(860, 270, k('cache'), 'cylinder', '#ef4444', 120, 100);
      return {
        nodes: [
          text(0, -70, `# ${k('name')}`, 18, 600),
          frame(260, 40, 520, 380, k('services')),
          frame(820, 40, 200, 380, k('data')),
          client, gateway, svcA, svcB, db, cache,
        ],
        edges: [line(client, gateway, 'HTTPS'), line(gateway, svcA), line(gateway, svcB), line(svcA, db), line(svcB, cache)],
      };
    },
  },
  {
    id: 'network',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'network');
      const internet = box(0, 150, k('internet'), 'netCloud', '#64748b', 120, 90);
      const firewall = box(200, 150, k('firewall'), 'netFirewall', '#ef4444', 90, 90);
      const router = box(370, 150, k('router'), 'netRouter', '#3b82f6', 90, 90);
      const sw = box(540, 150, k('switch'), 'netSwitch', '#3b82f6', 90, 90);
      const server = box(740, 0, k('server'), 'netServer', '#10b981', 90, 90);
      const desktop = box(740, 150, k('desktop'), 'netDesktop', '#64748b', 90, 90);
      const wifi = box(740, 300, k('wifi'), 'netAccessPoint', '#7c3aed', 90, 90);
      const laptop = box(920, 300, k('laptop'), 'netLaptop', '#64748b', 90, 90);
      return {
        nodes: [title(t, 'whiteboard.templates.network', -110), frame(500, -40, 560, 480, k('office')), internet, firewall, router, sw, server, desktop, wifi, laptop],
        edges: [
          link(internet, firewall, { arrow: false }), link(firewall, router, { arrow: false }), link(router, sw, { arrow: false }),
          ...[server, desktop, wifi].map((n) => link(sw, n, { arrow: false })),
          link(wifi, laptop, { arrow: false, dashed: true }),
        ],
      };
    },
  },
  {
    id: 'state_machine',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'state_machine');
      const begin = box(0, 35, '', 'umlInitial', '#1f2937', 30, 30);
      const idle = box(110, 10, k('idle'), 'umlState', '#3b82f6', 160, 80);
      const running = box(390, 10, k('running'), 'umlState', '#10b981', 160, 80);
      const paused = box(390, 220, k('paused'), 'umlState', '#f59e0b', 160, 80);
      const done = box(670, 10, k('done'), 'umlState', '#7c3aed', 160, 80);
      const end = box(910, 33, '', 'umlFinal', '#1f2937', 34, 34);
      return {
        nodes: [title(t, 'whiteboard.templates.state_machine', -80), begin, idle, running, paused, done, end],
        edges: [
          link(begin, idle), link(idle, running, { label: k('start') }), link(running, paused, { label: k('pause') }),
          link(paused, running, { label: k('resume'), sides: ['right', 'right'] }), link(running, done, { label: k('finish') }), link(done, end),
        ],
      };
    },
  },
  {
    id: 'sitemap',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'sitemap');
      const home = filled(470, 0, k('home'), 'wfBrowser', '#7c3aed', 200, 110);
      const sections = ['about', 'products', 'blog', 'contact'].map((key, i) => filled(i * 290, 200, k(key), 'roundedRect', '#3b82f6', 220, 64));
      const pages = [
        box(260, 330, k('product'), 'document', '#10b981', 180, 70),
        box(260, 430, k('pricing'), 'document', '#10b981', 180, 70),
        box(550, 330, k('post'), 'multiDocument', '#10b981', 180, 70),
      ];
      return {
        nodes: [title(t, 'whiteboard.templates.sitemap'), home, ...sections, ...pages],
        edges: [
          ...sections.map((s) => link(home, s, { arrow: false })),
          link(sections[1], pages[0], { arrow: false }), link(sections[1], pages[1], { arrow: false }), link(sections[2], pages[2], { arrow: false }),
        ],
      };
    },
  },
  {
    id: 'decision_tree',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'decision_tree');
      const decide = filled(0, 170, k('decision'), 'decision', '#f59e0b', 200, 120);
      const a = box(300, 60, k('option_a'), 'roundedRect', '#3b82f6', 180, 70);
      const b = box(300, 330, k('option_b'), 'roundedRect', '#3b82f6', 180, 70);
      const outcomes = ([['a_good', 0], ['a_bad', 110], ['b_good', 270], ['b_bad', 380]] as const)
        .map(([key, y]) => filled(640, y, k(key), 'ellipse', '#10b981', 190, 80));
      return {
        nodes: [title(t, 'whiteboard.templates.decision_tree', -80), decide, a, b, ...outcomes],
        edges: [
          link(decide, a, { label: k('yes') }), link(decide, b, { label: k('no') }),
          // Curves: two stepped lines from one box share a trunk, and their words sat on each other.
          link(a, outcomes[0], { label: k('likely'), curve: true }), link(a, outcomes[1], { label: k('unlikely'), curve: true }),
          link(b, outcomes[2], { label: k('likely'), curve: true }), link(b, outcomes[3], { label: k('unlikely'), curve: true }),
        ],
      };
    },
  },
  {
    id: 'venn',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'venn');
      const circle = (x: number, y: number, color: string) => box(x, y, '', 'ellipse', color, 360, 360, { fillColor: `${color}26` });
      return {
        nodes: [
          title(t, 'whiteboard.templates.venn'),
          circle(0, 0, '#3b82f6'), circle(220, 0, '#ec4899'), circle(110, 190, '#f59e0b'),
          ...[
            text(40, 140, `**${k('a')}**`, 15, 140),
            text(400, 140, `**${k('b')}**`, 15, 140),
            text(220, 440, `**${k('c')}**`, 15, 140),
            text(220, 220, k('all'), 13, 140),
          ].map((n) => ({ ...n, data: { ...n.data, textAlign: 'center' } })),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'concept_map',
    category: 'diagrams',
    build: (t) => {
      const k = words(t, 'concept_map');
      const main = filled(330, 0, k('main'), 'ellipse', '#7c3aed', 220, 90);
      const c1 = filled(0, 190, k('c1'), 'roundedRect', '#3b82f6', 190, 70);
      const c2 = filled(345, 210, k('c2'), 'roundedRect', '#3b82f6', 190, 70);
      const c3 = filled(690, 190, k('c3'), 'roundedRect', '#3b82f6', 190, 70);
      const e1 = box(0, 380, k('example'), 'roundedRect', '#10b981', 190, 64);
      const e2 = box(690, 380, k('result'), 'roundedRect', '#10b981', 190, 64);
      return {
        nodes: [title(t, 'whiteboard.templates.concept_map'), main, c1, c2, c3, e1, e2],
        edges: [
          link(main, c1, { label: k('includes'), curve: true }), link(main, c2, { label: k('needs') }),
          link(main, c3, { label: k('leads_to'), curve: true }), link(c1, e1, { label: k('for_example') }),
          link(c3, e2, { label: k('causes') }), link(c2, c3, { label: k('affects'), dashed: true }),
        ],
      };
    },
  },

  // Mind maps ───────────────────────────────────────────────
  {
    id: 'mindmap',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'mindmap');
      return mindmapOf(idea(k, 'root', ['b1', 'b2', 'b3', 'b4'].map((b) => idea(k, b))));
    },
  },
  {
    id: 'mm_5w1h',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'mm_5w1h');
      return mindmapOf(idea(k, 'root', ['what', 'why', 'who', 'when', 'where', 'how'].map((key) => idea(k, key, [idea(k, `${key}_hint`)]))));
    },
  },
  {
    id: 'mm_org',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'mm_org');
      return mindmapOf(idea(k, 'root', [
        idea(k, 'product', [idea(k, 'design'), idea(k, 'engineering')]),
        idea(k, 'marketing', [idea(k, 'content'), idea(k, 'ads')]),
        idea(k, 'operations', [idea(k, 'finance'), idea(k, 'people')]),
      ]), 'down');
    },
  },
  {
    id: 'logic_chart',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'logic_chart');
      return mindmapOf(idea(k, 'root', ['cause_1', 'cause_2', 'cause_3'].map((key) =>
        idea(k, key, [idea(k, 'evidence'), idea(k, 'fix')]))), 'right');
    },
  },
  {
    id: 'book_notes',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'book_notes');
      return mindmapOf(idea(k, 'root', [
        idea(k, 'ideas', [idea(k, 'idea_1'), idea(k, 'idea_2')]),
        idea(k, 'quotes', [idea(k, 'quote')]),
        idea(k, 'questions', [idea(k, 'question')]),
        idea(k, 'apply', [idea(k, 'try', [], { task: true })]),
        idea(k, 'about', [idea(k, 'author'), idea(k, 'why_read')]),
      ]));
    },
  },
  {
    id: 'goal_breakdown',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'goal_breakdown');
      return mindmapOf(idea(k, 'root', ['m1', 'm2', 'm3'].map((m) =>
        idea(k, m, [idea(k, 'step_1', [], { task: true }), idea(k, 'step_2', [], { task: true })]))), 'right');
    },
  },
  {
    id: 'study_topic',
    category: 'mindmaps',
    build: (t) => {
      const k = words(t, 'study_topic');
      return mindmapOf(idea(k, 'root', [
        idea(k, 'terms', [idea(k, 'term')]),
        idea(k, 'concepts', [idea(k, 'concept')]),
        idea(k, 'examples', [idea(k, 'example')]),
        idea(k, 'questions', [idea(k, 'question')]),
        idea(k, 'sources', [idea(k, 'source')]),
      ]));
    },
  },

  // Product and design ──────────────────────────────────────
  {
    id: 'kanban',
    category: 'product',
    build: (t) => ({
      nodes: [
        text(0, -90, `# ${t('whiteboard.templates.kanban.name')}`, 18, 600),
        ...columns(t, 'whiteboard.templates.kanban', ['todo', 'doing', 'done'], ['yellow', 'orange', 'green'], 320, 640),
      ],
      edges: [],
    }),
  },
  {
    id: 'product_roadmap',
    category: 'product',
    build: (t) => ({
      nodes: [
        title(t, 'whiteboard.templates.product_roadmap'),
        ...columns(t, 'whiteboard.templates.product_roadmap', ['now', 'next', 'later'], ['green', 'blue', 'purple'], 380, 560),
      ],
      edges: [],
    }),
  },
  {
    id: 'user_story_map',
    category: 'product',
    build: (t) => {
      const k = words(t, 'user_story_map');
      const nodes: WBNode[] = [title(t, 'whiteboard.templates.user_story_map', -110), text(0, -50, k('user'), 14, 1000)];
      nodes.push(frame(-20, 180, 1100, 230, k('release_1'), '#22c55e'), frame(-20, 430, 1100, 230, k('release_2'), '#3b82f6'));
      ['a1', 'a2', 'a3', 'a4'].forEach((key, i) => {
        const x = i * 270;
        nodes.push(filled(x, 0, k(key), 'pill', '#7c3aed', 240, 56));
        nodes.push(box(x, 86, k(`s${i + 1}`), 'roundedRect', '#3b82f6', 240, 60));
        nodes.push(sticky(x + 50, 225, i ? '' : k('story_hint'), 'yellow', 140));
        nodes.push(sticky(x + 50, 475, '', 'yellow', 140));
      });
      return { nodes, edges: [] };
    },
  },
  {
    id: 'empathy_map',
    category: 'product',
    build: (t) => {
      const k = words(t, 'empathy_map');
      return {
        nodes: [
          title(t, 'whiteboard.templates.empathy_map'),
          ...quadrants(t, 'whiteboard.templates.empathy_map', [['says', 'yellow'], ['thinks', 'blue'], ['does', 'green'], ['feels', 'pink']], 440, 300),
          filled(370, 250, k('user'), 'ellipse', '#7c3aed', 160, 120),
          frame(0, 660, 440, 220, k('pains')),
          sticky(40, 700, k('pains_hint'), 'orange', 150),
          frame(460, 660, 440, 220, k('gains')),
          sticky(500, 700, k('gains_hint'), 'green', 150),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'persona',
    category: 'product',
    build: (t) => {
      const k = words(t, 'persona');
      const parts: [string, string][] = [['goals', 'blue'], ['frustrations', 'pink'], ['motivations', 'green'], ['tools', 'purple']];
      return {
        nodes: [
          title(t, 'whiteboard.templates.persona'),
          frame(0, 0, 1000, 540, k('card')),
          box(40, 60, '', 'wfAvatar', '#7c3aed', 120, 120),
          text(190, 64, `## ${k('person')}`, 16, 760),
          text(190, 110, k('bio'), 14, 760),
          ...parts.flatMap(([key, c], i) => [
            text(40 + i * 235, 230, `**${k(key)}**`, 14, 200),
            sticky(40 + i * 235, 270, k(`${key}_hint`), c, 200),
          ]),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'customer_journey',
    category: 'product',
    build: (t) => {
      const k = words(t, 'customer_journey');
      const stages = ['aware', 'consider', 'buy', 'use', 'loyal'];
      const rows: [string, string][] = [['doing', 'blue'], ['thinking', 'purple'], ['feeling', 'yellow'], ['pains', 'pink'], ['ideas', 'green']];
      const nodes: WBNode[] = [title(t, 'whiteboard.templates.customer_journey')];
      stages.forEach((key, i) => nodes.push(filled(170 + i * 220, 0, k(key), 'chevron', '#7c3aed', 210, 56)));
      rows.forEach(([key, c], r) => {
        const y = 90 + r * 150;
        nodes.push(text(0, y + 50, `**${k(key)}**`, 14, 150));
        stages.forEach((_, i) => nodes.push(sticky(195 + i * 220, y, i ? '' : k(`${key}_hint`), c, 130)));
      });
      return { nodes, edges: [] };
    },
  },
  {
    id: 'wireframe_mobile',
    category: 'product',
    build: (t) => {
      const k = words(t, 'wireframe_mobile');
      // The bar and the rows draw lines where their words would go: words of
      // ours on them were struck through. The search box and the button take words.
      const ui = (x: number, y: number, w: number, h: number, label: string, shape: string) => box(x, y, label, shape, '#64748b', w, h);
      return {
        nodes: [
          title(t, 'whiteboard.templates.wireframe_mobile'),
          ui(0, 0, 320, 640, '', 'wfPhone'),
          ui(20, 50, 280, 48, '', 'wfNavBar'),
          ui(32, 112, 256, 40, k('search'), 'wfSearch'),
          ui(32, 166, 256, 140, '', 'wfImage'),
          ...[0, 1, 2].map((i) => ui(32, 320 + i * 62, 256, 54, '', 'wfListItem')),
          ui(32, 510, 256, 46, k('action'), 'wfButton'),
          ui(20, 570, 280, 50, '', 'wfTabBar'),
          sticky(380, 40, k('note_hint'), 'yellow', 180),
          sticky(380, 260, k('flow_hint'), 'blue', 180),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'sprint_planning',
    category: 'product',
    build: (t) => {
      const k = words(t, 'sprint_planning');
      return {
        nodes: [
          title(t, 'whiteboard.templates.sprint_planning'),
          frame(0, 0, 1040, 150, k('goal')),
          text(30, 60, k('goal_hint'), 18, 980),
          ...columns(t, 'whiteboard.templates.sprint_planning', ['backlog', 'sprint', 'risks'], ['yellow', 'blue', 'orange'], 320, 460, 190),
        ],
        edges: [],
      };
    },
  },

  // Personal and learning ───────────────────────────────────
  {
    id: 'weekly_review',
    category: 'personal',
    build: (t) => {
      const k = words(t, 'weekly_review');
      // The first two columns are the Tasks app's own answer, kept up to date
      // as the week goes on — see `useLiveFrames`.
      const live = (x: number, label: string, query: string): WBNode => ({ ...frame(x, 0, 608, 480, label), data: { label, width: 608, height: 480, query } });
      return {
        nodes: [
          text(0, -90, `# ${k('name')}`, 18, 600),
          live(0, k('done'), `is:task status:done completed_at:>=${weekStart()} sort:-completed_at`),
          live(648, k('carry'), `is:task status:todo due_date:<=${today()} sort:due_date`),
          ...columns(t, 'whiteboard.templates.weekly_review', ['wins', 'learned', 'next'], ['yellow', 'blue', 'purple'], 300, 480)
            .map((n) => ({ ...n, position: { x: n.position.x + 1296, y: n.position.y } })),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'weekly_planner',
    category: 'personal',
    build: (t) => {
      const k = words(t, 'weekly_planner');
      const days = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
      return {
        nodes: [
          title(t, 'whiteboard.templates.weekly_planner'),
          ...days.map((d, i) => frame(i * 230, 0, 210, 420, k(d), i > 4 ? '#a855f7' : undefined)),
          sticky(15, 50, k('day_hint'), 'yellow', 180),
          frame(0, 460, 790, 240, k('priorities')),
          ...[0, 1, 2].map((i) => sticky(30 + i * 250, 500, i ? '' : k('priorities_hint'), 'green', 170)),
          frame(820, 460, 790, 240, k('notes')),
          sticky(850, 500, k('notes_hint'), 'blue', 170),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'daily_planner',
    category: 'personal',
    build: (t) => {
      const k = words(t, 'daily_planner');
      return {
        nodes: [
          title(t, 'whiteboard.templates.daily_planner'),
          frame(0, 0, 420, 240, k('top3')),
          ...[0, 1, 2].map((i) => sticky(20 + i * 130, 60, i ? '' : k('top3_hint'), 'yellow', 120)),
          ...block(t, 'whiteboard.templates.daily_planner', 'schedule', 0, 260, 420, 540),
          ...block(t, 'whiteboard.templates.daily_planner', 'todo', 440, 0, 400, 400),
          frame(440, 420, 400, 380, k('notes')),
          sticky(480, 470, k('notes_hint'), 'blue', 170),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'cornell_notes',
    category: 'personal',
    build: (t) => {
      const p = 'whiteboard.templates.cornell_notes';
      return {
        nodes: [
          title(t, p),
          ...block(t, p, 'cues', 0, 0, 300, 600),
          ...block(t, p, 'notes', 320, 0, 700, 600),
          ...block(t, p, 'summary', 0, 620, 1020, 200),
        ],
        edges: [],
      };
    },
  },
];

/**
 * A nine-part canvas, as the Business Model and Lean canvases are drawn: five
 * columns, the second and fourth split in two, and two wide blocks beneath.
 * `keys` go column by column, top to bottom, then the two blocks.
 */
function canvas(t: T, prefix: string, keys: string[]): WBNode[] {
  const [c1, c2a, c2b, c3, c4a, c4b, c5, low1, low2] = keys;
  const w = 240;
  const x = (col: number) => col * (w + 20);
  return [
    ...block(t, prefix, c1, x(0), 0, w, 480),
    ...block(t, prefix, c2a, x(1), 0, w, 230),
    ...block(t, prefix, c2b, x(1), 250, w, 230),
    ...block(t, prefix, c3, x(2), 0, w, 480),
    ...block(t, prefix, c4a, x(3), 0, w, 230),
    ...block(t, prefix, c4b, x(3), 250, w, 230),
    ...block(t, prefix, c5, x(4), 0, w, 480),
    ...block(t, prefix, low1, 0, 500, 640, 220),
    ...block(t, prefix, low2, 660, 500, 640, 220),
  ];
}
