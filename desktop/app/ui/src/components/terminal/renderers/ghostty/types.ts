// The render state libghostty-vt gives the renderer, as plain data: rows of
// cells with their text, colours and styles, the cursor and the colours.
//
// From T3 Code (https://github.com/pingdotgg/t3code), MIT licence, Copyright
// (c) 2026 T3 Tools Inc., apps/web/src/terminal/ghostty/core.ts at commit
// ade47301. The licence is in licenses/t3code.txt.

export interface GhosttyColor {
  readonly r: number;
  readonly g: number;
  readonly b: number;
}

export interface GhosttyTheme {
  readonly foreground: GhosttyColor;
  readonly background: GhosttyColor;
  readonly cursor: GhosttyColor;
  readonly palette?: readonly GhosttyColor[];
  readonly selectionBackground?: string;
}

export interface GhosttyCell {
  readonly text: string;
  readonly wide: number;
  readonly foreground: GhosttyColor;
  readonly background: GhosttyColor;
  readonly bold: boolean;
  readonly italic: boolean;
  readonly invisible: boolean;
  readonly strikethrough: boolean;
  readonly overline: boolean;
  readonly underline: boolean;
  readonly selected: boolean;
}

export interface GhosttyRow {
  readonly cells: readonly GhosttyCell[];
  readonly text: string;
  readonly isWrapContinuation: boolean;
  readonly wrapsToNext: boolean;
}

export interface GhosttySnapshot {
  readonly cols: number;
  readonly rows: number;
  readonly foreground: GhosttyColor;
  readonly background: GhosttyColor;
  readonly cursor: GhosttyColor;
  readonly cursorX: number;
  readonly cursorY: number;
  readonly cursorVisible: boolean;
  readonly cursorBlinking: boolean;
  readonly cursorStyle: number;
  readonly dirtyRows: ReadonlySet<number>;
  readonly rowData: readonly GhosttyRow[];
}
