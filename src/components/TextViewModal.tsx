import { useEffect } from "react";
import { CopyButton } from "./CopyButton";

interface Props {
  title: string;
  subtitle?: string;
  text: string;
  loading?: boolean;
  error?: string | null;
  onClose: () => void;
}

/** Read-only modal showing monospace text (e.g. `describe` output) with copy. */
export function TextViewModal({ title, subtitle, text, loading, error, onClose }: Props) {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="modal text-view" onMouseDown={(e) => e.stopPropagation()}>
        <div className="text-view-head">
          <div>
            <div className="modal-title">{title}</div>
            {subtitle && (
              <div className="dim mono" style={{ fontSize: "0.82em", marginTop: 2 }}>
                {subtitle}
              </div>
            )}
          </div>
          {!loading && !error && <CopyButton text={text} title="Copy" label="Copy" />}
        </div>

        {loading && <div className="dim" style={{ marginTop: 14 }}>Loading…</div>}
        {error && <div className="modal-error">⚠ {error}</div>}
        {!loading && !error && <pre className="manifest text-view-body">{text}</pre>}

        <div className="modal-actions">
          <button className="btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
