import { useRef, useState } from "react";

/** Copy text to the clipboard; falls back to execCommand for older WebViews. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through to legacy path */
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}

interface Props {
  /** Text to copy, or a producer for text that's expensive to build. */
  text: string | (() => string);
  /** Tooltip; defaults to "Copy". */
  title?: string;
  /** Optional visible label next to the icon (e.g. "Copy"). */
  label?: string;
  className?: string;
}

/** Small copy-to-clipboard button with transient ✓ feedback. */
export function CopyButton({ text, title, label, className }: Props) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  async function onCopy(e: React.MouseEvent) {
    e.stopPropagation();
    const value = typeof text === "function" ? text() : text;
    if (await copyText(value)) {
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 1200);
    }
  }

  return (
    <button
      className={`copy-btn${copied ? " copied" : ""}${className ? ` ${className}` : ""}`}
      title={copied ? "Copied!" : title ?? "Copy"}
      onClick={onCopy}
    >
      {copied ? "✓" : "⧉"}
      {label ? <span className="copy-label">{copied ? "Copied" : label}</span> : null}
    </button>
  );
}
