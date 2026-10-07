/**
 * Option colours. The file names a colour (`blue`); how it looks in light and
 * dark is decided here, through two custom properties the stylesheet reads.
 */
import { OPTION_COLORS, type Column } from './model';

const PALETTE: Record<string, { light: [string, string]; dark: [string, string] }> = {
  gray: { light: ['#f1f1ef', '#37352f'], dark: ['#3a3a3a', '#e3e3e3'] },
  red: { light: ['#fde2e1', '#9b2c2c'], dark: ['#5c2626', '#ffd6d4'] },
  orange: { light: ['#fde9d7', '#9a4a12'], dark: ['#5c3519', '#ffe0c2'] },
  yellow: { light: ['#fbf2c9', '#7a5d0b'], dark: ['#564718', '#fff0b3'] },
  green: { light: ['#dcf2e3', '#1f6b3a'], dark: ['#1f4a2f', '#c9f2d6'] },
  blue: { light: ['#dbeafd', '#1d4f91'], dark: ['#1d3a5f', '#cfe3ff'] },
  purple: { light: ['#ece4fb', '#5b3699'], dark: ['#3d2b5f', '#e6d9ff'] },
  pink: { light: ['#fbe3ef', '#8f2b5c'], dark: ['#5a2540', '#ffd6ea'] },
};

/**
 * The colour of an option: the one the column gives it, else one picked by
 * the option's place in the list, so a new option is never all grey.
 */
export function colorName(column: Column, option: string): string {
  const named = column.colors?.[option];
  if (named && PALETTE[named]) return named;
  const at = column.options?.indexOf(option) ?? -1;
  if (at === -1) return 'gray';
  return OPTION_COLORS[(at % (OPTION_COLORS.length - 1)) + 1];
}

export function optionColor(column: Column, option: string): Record<string, string> {
  const { light, dark } = PALETTE[colorName(column, option)];
  return {
    '--rt-chip-bg': light[0],
    '--rt-chip-fg': light[1],
    '--rt-chip-bg-dark': dark[0],
    '--rt-chip-fg-dark': dark[1],
  };
}
