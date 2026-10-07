/** One line of a Rich Table's body: a row, or — when the view groups — a group's heading or subtotal. */
export type BodyItem =
  | { kind: 'row'; ri: number; di: number }
  | { kind: 'group'; key: string; label: string; count: number; collapsed: boolean; color?: Record<string, string> }
  | { kind: 'subtotal'; key: string; values: (string | null)[] };
