/**
 * Boards to start from.
 *
 * A blank board is a fine start for a sketch and a slow one for a meeting
 * that always has the same shape: a weekly review, a kickoff, a retro. These
 * are those shapes, laid out with frames and sticky notes the way a person
 * would lay them out by hand, with the words in the user's language.
 *
 * Each builds items around (0, 0); the board places them where they go and
 * gives them ids of its own.
 */
import type { WBEdge, WBNode } from './boardFile';
import { today, weekStart } from './vaultCards';

type T = (key: string) => string;

export interface BoardTemplate {
  id: string;
  /** i18n keys under `whiteboard.templates.<id>`: `name`, `desc`, and the words on the board. */
  build: (t: T) => { nodes: WBNode[]; edges: WBEdge[] };
}

let seq = 0;
const id = (kind: string) => `tpl-${kind}-${++seq}`;

const frame = (x: number, y: number, width: number, height: number, label: string, color?: string): WBNode => ({
  id: id('frame'), type: 'frame', position: { x, y }, data: { label, width, height, ...(color ? { color } : {}) },
});
const sticky = (x: number, y: number, label: string, color = 'yellow', size = 180): WBNode => ({
  id: id('sticky'), type: 'sticky', position: { x, y }, data: { label, color, width: size, height: size },
});
const text = (x: number, y: number, label: string, fontSize = 16, width = 320): WBNode => ({
  id: id('text'), type: 'text', position: { x, y }, data: { label, fontSize, width },
});
const box = (x: number, y: number, label: string, shapeType = 'roundedRect', color = '#3b82f6', width = 160, height = 72): WBNode => ({
  id: id('shape'), type: 'shape', position: { x, y }, data: { shapeType, label, color, width, height },
});
const line = (source: WBNode, target: WBNode, label?: string): WBEdge => ({
  id: id('edge'), source: source.id, target: target.id, type: 'default',
  sourceHandle: 'right', targetHandle: 'left',
  data: { markerEnd: 'arrow', ...(label ? { label } : {}) },
});

/** A row of frames, each with one sticky note inviting the first entry. */
function columns(t: T, prefix: string, keys: string[], colors: string[], width = 320, height = 520) {
  const nodes: WBNode[] = [];
  keys.forEach((key, i) => {
    const x = i * (width + 40);
    nodes.push(frame(x, 0, width, height, t(`${prefix}.${key}`)));
    nodes.push(sticky(x + (width - 180) / 2, 40, t(`${prefix}.${key}_hint`), colors[i % colors.length]));
  });
  return nodes;
}

export const TEMPLATES: BoardTemplate[] = [
  {
    id: 'weekly_review',
    build: (t) => {
      const k = (s: string) => t(`whiteboard.templates.weekly_review.${s}`);
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
    id: 'project_kickoff',
    build: (t) => {
      const k = (s: string) => t(`whiteboard.templates.project_kickoff.${s}`);
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
    id: 'system_diagram',
    build: (t) => {
      const k = (s: string) => t(`whiteboard.templates.system_diagram.${s}`);
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
    id: 'retro',
    build: (t) => ({
      nodes: [
        text(0, -90, `# ${t('whiteboard.templates.retro.name')}`, 18, 600),
        ...columns(t, 'whiteboard.templates.retro', ['well', 'improve', 'actions'], ['green', 'pink', 'blue'], 360, 520),
      ],
      edges: [],
    }),
  },
  {
    id: 'kanban',
    build: (t) => ({
      nodes: [
        text(0, -90, `# ${t('whiteboard.templates.kanban.name')}`, 18, 600),
        ...columns(t, 'whiteboard.templates.kanban', ['todo', 'doing', 'done'], ['yellow', 'orange', 'green'], 320, 640),
      ],
      edges: [],
    }),
  },
  {
    id: 'swot',
    build: (t) => {
      const k = (s: string) => t(`whiteboard.templates.swot.${s}`);
      const cells: [string, string, number, number][] = [
        ['strengths', 'green', 0, 0], ['weaknesses', 'pink', 1, 0],
        ['opportunities', 'blue', 0, 1], ['threats', 'orange', 1, 1],
      ];
      return {
        nodes: [
          text(0, -90, `# ${k('name')}`, 18, 600),
          ...cells.flatMap(([key, color, cx, cy]) => [
            frame(cx * 440, cy * 340, 420, 320, k(key)),
            sticky(cx * 440 + 40, cy * 340 + 50, k(`${key}_hint`), color, 170),
          ]),
        ],
        edges: [],
      };
    },
  },
  {
    id: 'mindmap',
    build: (t) => {
      const k = (s: string) => t(`whiteboard.templates.mindmap.${s}`);
      const root: WBNode = { id: id('mind'), type: 'mindmap', position: { x: 0, y: 0 }, data: { label: k('root'), level: 0, color: '#7c3aed' } };
      const colors = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444'];
      const kids = ['b1', 'b2', 'b3', 'b4'].map((key, i): WBNode => ({
        id: id('mind'), type: 'mindmap', position: { x: i < 2 ? 260 : -260, y: (i % 2) * 120 - 60 },
        data: { label: k(key), level: 1, color: colors[i], direction: i < 2 ? 'right' : 'left' },
      }));
      return {
        nodes: [root, ...kids],
        edges: kids.map((kid) => ({
          id: id('edge'), source: root.id, target: kid.id, type: 'default', data: {},
          sourceHandle: kid.data.direction === 'left' ? 'left-source' : 'right-source',
          targetHandle: kid.data.direction === 'left' ? 'right-target' : 'left-target',
        })),
      };
    },
  },
];
