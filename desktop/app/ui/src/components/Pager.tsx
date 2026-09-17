// The pager under a list: where this page sits in the whole, the way to the
// pages either side, and how many rows a page holds. Hidden when everything
// fits on one page at the smallest size.
//
// Previous and Next ask for a step rather than a page number: a second click
// arrives before this pager has been drawn again, and a number worked out
// here would be worked out from the page already left behind.

import { ChevronLeft, ChevronRight } from "lucide-react";
import { Select } from "./Select";

export const PAGE_SIZES = [25, 50, 100] as const;
export const DEFAULT_PAGE_SIZE = 50;

/** A page size from the URL, or the default when it is not one of the sizes. */
export const pageSizeOf = (raw: string | null) => {
  const n = Number(raw);
  return (PAGE_SIZES as readonly number[]).includes(n) ? n : DEFAULT_PAGE_SIZE;
};

/** A 1-based page number from the URL, at least 1. */
export const pageOf = (raw: string | null) => Math.max(1, Math.floor(Number(raw)) || 1);

type Props = {
  /** 1-based */
  page: number;
  size: number;
  total: number;
  /** a page number, or a step from the page the URL is on */
  onPage: (page: number | ((current: number) => number)) => void;
  onSize: (size: number) => void;
  label: string;
};

export function Pager({ page, size, total, onPage, onSize, label }: Props) {
  if (total <= PAGE_SIZES[0]) return null;
  const pages = Math.max(1, Math.ceil(total / size));
  const first = total === 0 ? 0 : (page - 1) * size + 1;
  const last = Math.min(total, page * size);
  return (
    <nav className="pager" aria-label={`${label} pages`} data-pager>
      <span className="pager-range" data-pager-range>
        {first}–{last} of {total}
      </span>
      <span className="pager-steps">
        <button type="button" className="chrome-button" onClick={() => onPage((p) => p - 1)} disabled={page <= 1} aria-label="Previous page" data-pager-previous>
          <ChevronLeft size={14} />
          Previous
        </button>
        <span className="pager-page">
          Page {page} of {pages}
        </span>
        <button type="button" className="chrome-button" onClick={() => onPage((p) => p + 1)} disabled={page >= pages} aria-label="Next page" data-pager-next>
          Next
          <ChevronRight size={14} />
        </button>
      </span>
      <Select
        label="Rows per page"
        value={String(size)}
        onChange={(v) => onSize(Number(v))}
        options={PAGE_SIZES.map((n) => ({ value: String(n), label: `${n} per page` }))}
      />
    </nav>
  );
}
