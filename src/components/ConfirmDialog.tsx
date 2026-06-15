import { useEffect, useState } from "react";

export interface ConfirmRequest {
  title: string;
  message: string;
  /** Confirm button label, e.g. "Delete" / "Restart". */
  confirmLabel: string;
  /** Danger styles the confirm button red (deletes). */
  danger?: boolean;
  /** When set, the user must type this exact string to enable confirm
   *  (extra guard for cascading deletes like namespaces). */
  confirmText?: string;
  action: () => Promise<void>;
}

interface Props {
  req: ConfirmRequest;
  onClose: () => void;
}

/** Modal confirmation for destructive actions (native dialogs block the WebView). */
export function ConfirmDialog({ req, onClose }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [typed, setTyped] = useState("");

  const confirmGated = req.confirmText !== undefined && typed !== req.confirmText;

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function onConfirm() {
    setBusy(true);
    setError(null);
    try {
      await req.action();
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !busy && onClose()}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">{req.title}</div>
        <div className="modal-body">{req.message}</div>
        {req.confirmText !== undefined && (
          <input
            className="input"
            style={{ marginTop: 12, width: "100%" }}
            placeholder={`Type "${req.confirmText}" to confirm`}
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            disabled={busy}
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter" && !confirmGated && !busy) onConfirm();
            }}
          />
        )}
        {error && <div className="modal-error">⚠ {error}</div>}
        <div className="modal-actions">
          <button className="btn" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            className={`btn ${req.danger ? "danger" : "primary"}`}
            onClick={onConfirm}
            disabled={busy || confirmGated}
            autoFocus={req.confirmText === undefined}
          >
            {busy ? "Working…" : req.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
