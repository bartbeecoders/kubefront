import { useEffect, useRef, useState } from "react";

export interface LogLine {
  text: string;
  err: boolean;
}

export interface LogWindowState {
  id: number;
  streamId: number | null;
  pod: string;
  namespace: string;
  containers: string[];
  selectedContainer: string | null;
  lines: LogLine[];
  follow: boolean;
  filter: string;
}

interface Props {
  win: LogWindowState;
  index: number;
  onClose: (id: number) => void;
  onPatch: (id: number, patch: Partial<LogWindowState>) => void;
  onChangeContainer: (id: number, container: string) => void;
}

export function LogWindow({ win, index, onClose, onPatch, onChangeContainer }: Props) {
  const [pos, setPos] = useState({ x: 120 + index * 28, y: 90 + index * 28 });
  const drag = useRef<{ dx: number; dy: number } | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when following.
  useEffect(() => {
    if (win.follow && bodyRef.current) {
      bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
    }
  }, [win.lines, win.follow]);

  useEffect(() => {
    function move(e: MouseEvent) {
      if (!drag.current) return;
      setPos({ x: e.clientX - drag.current.dx, y: e.clientY - drag.current.dy });
    }
    function up() {
      drag.current = null;
    }
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, []);

  const lower = win.filter.toLowerCase();
  const visible = win.filter
    ? win.lines.filter((l) => l.text.toLowerCase().includes(lower))
    : win.lines;

  const title = win.selectedContainer
    ? `Logs: ${win.namespace}/${win.pod} [${win.selectedContainer}]`
    : `Logs: ${win.namespace}/${win.pod}`;

  return (
    <div className="logwin" style={{ left: pos.x, top: pos.y }}>
      <div
        className="logwin-head"
        onMouseDown={(e) => {
          drag.current = { dx: e.clientX - pos.x, dy: e.clientY - pos.y };
        }}
      >
        <span className="title">{title}</span>
        <button className="btn ghost sm" onClick={() => onClose(win.id)}>
          ✕
        </button>
      </div>

      <div className="logwin-toolbar">
        <label className="checkbox">
          <input
            type="checkbox"
            checked={win.follow}
            onChange={(e) => onPatch(win.id, { follow: e.target.checked })}
          />
          Follow
        </label>

        {win.containers.length > 1 && (
          <select
            className="select"
            value={win.selectedContainer ?? ""}
            onChange={(e) => onChangeContainer(win.id, e.target.value)}
          >
            {win.containers.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        )}

        <input
          className="input"
          placeholder="filter…"
          value={win.filter}
          onChange={(e) => onPatch(win.id, { filter: e.target.value })}
          style={{ flex: 1, minWidth: 80 }}
        />

        <button
          className="btn sm"
          onClick={() => {
            const text = visible.map((l) => l.text).join("\n");
            navigator.clipboard?.writeText(text).catch(() => {});
          }}
        >
          Copy
        </button>
        <button className="btn sm" onClick={() => onPatch(win.id, { lines: [] })}>
          Clear
        </button>
      </div>

      <div className="logwin-body" ref={bodyRef}>
        {visible.map((l, i) => (
          <div key={i} className={l.err ? "logline err" : "logline"}>
            {l.text}
          </div>
        ))}
      </div>
    </div>
  );
}
