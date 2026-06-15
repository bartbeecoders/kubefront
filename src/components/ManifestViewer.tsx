import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { CopyButton } from "./CopyButton";

interface Props {
  /** The manifest text (JSON/YAML) to display. */
  text: string;
  /** Title shown in the fullscreen overlay header (e.g. the resource name). */
  title: string;
}

type Segment = { value: string; matchIndex: number | null };

/** Split `text` into segments around case-insensitive matches of `query`,
 *  numbering each match so it can be highlighted and scrolled to. */
function segment(text: string, query: string): { segments: Segment[]; count: number } {
  if (!query) return { segments: [{ value: text, matchIndex: null }], count: 0 };
  const segments: Segment[] = [];
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  let i = 0;
  let count = 0;
  while (i < text.length) {
    const next = lower.indexOf(q, i);
    if (next === -1) {
      segments.push({ value: text.slice(i), matchIndex: null });
      break;
    }
    if (next > i) segments.push({ value: text.slice(i, next), matchIndex: null });
    segments.push({ value: text.slice(next, next + q.length), matchIndex: count });
    count += 1;
    i = next + q.length;
  }
  return { segments, count };
}

/** Read-only manifest viewer with in-text search (highlight + next/prev) and a
 *  fullscreen toggle. Used for resource YAML/JSON in the detail panel. */
export function ManifestViewer({ text, title }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [fullscreen, setFullscreen] = useState(false);

  const { segments, count } = useMemo(() => segment(text, query), [text, query]);

  // Keep the active match in range as the query (and thus match count) changes.
  useEffect(() => {
    setActive((a) => (count === 0 ? 0 : Math.min(a, count - 1)));
  }, [count]);

  // Esc closes fullscreen.
  useEffect(() => {
    if (!fullscreen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setFullscreen(false);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [fullscreen]);

  const step = (delta: number) => {
    if (count === 0) return;
    setActive((a) => (a + delta + count) % count);
  };

  const body = (
    <ManifestBody segments={segments} active={count === 0 ? -1 : active} />
  );

  const search = (
    <div className="manifest-search">
      <input
        className="input sm"
        placeholder="Search…"
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") step(e.shiftKey ? -1 : 1);
        }}
      />
      <span className="manifest-search-count dim">
        {query ? (count === 0 ? "0/0" : `${active + 1}/${count}`) : ""}
      </span>
      <button className="btn sm" title="Previous match" disabled={count === 0} onClick={() => step(-1)}>
        ↑
      </button>
      <button className="btn sm" title="Next match" disabled={count === 0} onClick={() => step(1)}>
        ↓
      </button>
    </div>
  );

  if (fullscreen) {
    return (
      <div className="modal-backdrop" onMouseDown={() => setFullscreen(false)}>
        <div className="modal manifest-full" onMouseDown={(e) => e.stopPropagation()}>
          <div className="manifest-full-head">
            <div className="modal-title" title={title}>
              {title}
            </div>
            <div className="manifest-full-tools">
              {search}
              <CopyButton text={text} title="Copy" label="Copy" />
              <button className="btn sm" onClick={() => setFullscreen(false)}>
                ✕ Exit
              </button>
            </div>
          </div>
          {body}
        </div>
      </div>
    );
  }

  return (
    <div className="manifest-viewer">
      <div className="manifest-toolbar">
        {search}
        <button className="btn sm" title="Fullscreen" onClick={() => setFullscreen(true)}>
          ⤢ Fullscreen
        </button>
      </div>
      {body}
    </div>
  );
}

function ManifestBody({ segments, active }: { segments: Segment[]; active: number }) {
  const activeRef = useRef<HTMLElement>(null);

  // Scroll the active match into view whenever it changes.
  useLayoutEffect(() => {
    activeRef.current?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [active]);

  return (
    <pre className="manifest">
      {segments.map((s, i) =>
        s.matchIndex === null ? (
          <span key={i}>{s.value}</span>
        ) : (
          <mark
            key={i}
            ref={s.matchIndex === active ? activeRef : undefined}
            className={`manifest-match${s.matchIndex === active ? " active" : ""}`}
          >
            {s.value}
          </mark>
        ),
      )}
    </pre>
  );
}
