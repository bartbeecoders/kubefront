// Column sorting + text filtering shared by every resource table
// (generic ResourceTable plus the bespoke Pods and Nodes views).

import { useState } from "react";

export type SortDir = "asc" | "desc";

export interface SortState {
  col: number;
  dir: SortDir;
}

/** Age strings produced by kube-core's format_age: "5d", "3h", "12m", "45s", "<1s". */
const AGE_RE = /^(<?)(\d+)([smhd])$/;
const AGE_UNIT_SECONDS: Record<string, number> = { s: 1, m: 60, h: 3600, d: 86400 };

function parseAge(s: string): number | null {
  const m = AGE_RE.exec(s);
  if (!m) return null;
  return Number(m[2]) * AGE_UNIT_SECONDS[m[3]];
}

/** Resource quantities like "10Gi", "500Mi", "1Ti" → bytes (also bare numbers). */
const QTY_RE = /^(\d+(?:\.\d+)?)([KMGTPE]i?)?$/;
const QTY_FACTOR: Record<string, number> = {
  K: 1e3,
  M: 1e6,
  G: 1e9,
  T: 1e12,
  P: 1e15,
  E: 1e18,
  Ki: 2 ** 10,
  Mi: 2 ** 20,
  Gi: 2 ** 30,
  Ti: 2 ** 40,
  Pi: 2 ** 50,
  Ei: 2 ** 60,
};

function parseQuantity(s: string): number | null {
  const m = QTY_RE.exec(s);
  if (!m) return null;
  return Number(m[1]) * (m[2] ? QTY_FACTOR[m[2]] : 1);
}

/**
 * Order two cell values: ages and quantities/plain numbers compare numerically,
 * everything else falls back to a natural (digit-aware) string compare.
 * Empty cells always sort last so meaningful values stay on top.
 */
export function compareCells(a: string, b: string): number {
  if (a === b) return 0;
  if (a === "") return 1;
  if (b === "") return -1;

  const aAge = parseAge(a);
  const bAge = parseAge(b);
  if (aAge !== null && bAge !== null) return aAge - bAge;

  const aQty = parseQuantity(a);
  const bQty = parseQuantity(b);
  if (aQty !== null && bQty !== null) return aQty - bQty;

  return a.localeCompare(b, undefined, { numeric: true, sensitivity: "base" });
}

/** Click cycle per column: ascending → descending → back to server order. */
export function useTableSort() {
  const [sort, setSort] = useState<SortState | null>(null);
  const toggle = (col: number) =>
    setSort((s) => {
      if (!s || s.col !== col) return { col, dir: "asc" };
      return s.dir === "asc" ? { col, dir: "desc" } : null;
    });
  return { sort, toggle };
}

/** Stable-sort rows by the active column; `cell` extracts a column's text from a row. */
export function sortRows<T>(
  rows: T[],
  sort: SortState | null,
  cell: (row: T, col: number) => string,
): T[] {
  if (!sort) return rows;
  const sign = sort.dir === "asc" ? 1 : -1;
  return [...rows].sort((a, b) => sign * compareCells(cell(a, sort.col), cell(b, sort.col)));
}

/** Case-insensitive substring match against any cell of a row. */
export function rowMatches(cells: string[], query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return cells.some((c) => c.toLowerCase().includes(q));
}

/** "▲" / "▼" for the active sort column, empty otherwise (rendered inside the th). */
export function sortMark(sort: SortState | null, col: number): string {
  if (!sort || sort.col !== col) return "";
  return sort.dir === "asc" ? "▲" : "▼";
}

export function ariaSort(
  sort: SortState | null,
  col: number,
): "ascending" | "descending" | undefined {
  if (!sort || sort.col !== col) return undefined;
  return sort.dir === "asc" ? "ascending" : "descending";
}
