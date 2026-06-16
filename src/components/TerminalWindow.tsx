import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { api } from "../api";

interface Props {
  /** Frontend window id (used for stacking offset + close). */
  winId: number;
  index: number;
  onClose: (winId: number) => void;
}

/** A floating, draggable, resizable terminal backed by a real PTY in Rust and
 *  rendered with xterm.js. Manages its own lifetime: opens the PTY on mount and
 *  closes it on unmount. */
export function TerminalWindow({ winId, index, onClose }: Props) {
  const [pos, setPos] = useState({ x: 160 + index * 28, y: 110 + index * 28 });
  const drag = useRef<{ dx: number; dy: number } | null>(null);

  const wrapRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const backendId = useRef<number | null>(null);

  // Create the xterm instance and open the PTY exactly once.
  useEffect(() => {
    const term = new Terminal({
      fontFamily: '"JetBrains Mono", ui-monospace, Menlo, Consolas, monospace',
      fontSize: 13,
      cursorBlink: true,
      // Conventional dark terminal palette (independent of the app light/dark theme).
      theme: { background: "#0b0e14", foreground: "#e8ebf2", cursor: "#e8ebf2" },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    termRef.current = term;
    fitRef.current = fit;
    if (hostRef.current) term.open(hostRef.current);
    try {
      fit.fit();
    } catch {
      /* host not laid out yet; ResizeObserver will refit */
    }

    let disposed = false;
    api
      .openTerminal(term.cols, term.rows, (e) => {
        if (e.event === "output") {
          term.write(new Uint8Array(e.data));
        } else {
          term.write("\r\n\x1b[2;33m[process exited]\x1b[0m");
        }
      })
      .then((id) => {
        if (disposed) {
          api.closeTerminal(id).catch(() => {});
          return;
        }
        backendId.current = id;
        // Reconcile the PTY size with the (possibly already refitted) viewport.
        api.resizeTerminal(id, term.cols, term.rows).catch(() => {});
      })
      .catch((err) => {
        term.write(`\r\n\x1b[31mFailed to open terminal: ${String(err)}\x1b[0m`);
      });

    const sub = term.onData((data) => {
      if (backendId.current != null) api.writeTerminal(backendId.current, data).catch(() => {});
    });
    term.focus();

    return () => {
      disposed = true;
      sub.dispose();
      if (backendId.current != null) api.closeTerminal(backendId.current).catch(() => {});
      term.dispose();
    };
  }, []);

  // Refit (and resize the PTY) whenever the window is resized.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const term = termRef.current;
      const fit = fitRef.current;
      if (!term || !fit) return;
      try {
        fit.fit();
        if (backendId.current != null) {
          api.resizeTerminal(backendId.current, term.cols, term.rows).catch(() => {});
        }
      } catch {
        /* ignore transient layout errors */
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Window drag (from the title bar).
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

  return (
    <div className="termwin" ref={wrapRef} style={{ left: pos.x, top: pos.y }}>
      <div
        className="termwin-head"
        onMouseDown={(e) => {
          drag.current = { dx: e.clientX - pos.x, dy: e.clientY - pos.y };
        }}
      >
        <span className="title">⌨ Terminal</span>
        <button className="btn ghost sm" onClick={() => onClose(winId)}>
          ✕
        </button>
      </div>
      <div className="termwin-body" onMouseDown={() => termRef.current?.focus()}>
        <div className="termwin-host" ref={hostRef} />
      </div>
    </div>
  );
}
