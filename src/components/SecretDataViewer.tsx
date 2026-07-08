import { useMemo } from "react";
import { CopyButton } from "./CopyButton";

interface Props {
  /** The Secret's full manifest JSON (as returned in ResourceDetail.manifest). */
  manifest: string;
}

interface DecodedEntry {
  key: string;
  /** The value to show (UTF-8 text, or raw base64 when the bytes aren't text). */
  value: string;
  /** Number of decoded bytes — handy for binary blobs. */
  bytes: number;
  /** True when the value couldn't be decoded as UTF-8 text (likely binary). */
  binary: boolean;
  /** Set when the field couldn't be base64-decoded at all. */
  error?: string;
}

/** Base64-decode a Secret `.data` value, recovering UTF-8 text where possible.
 *  Returns the raw base64 (flagged `binary`) when the bytes aren't valid text. */
function decodeValue(b64: string): { value: string; bytes: number; binary: boolean; error?: string } {
  let bytes: Uint8Array;
  try {
    const bin = atob(b64.replace(/\s/g, ""));
    bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  } catch {
    return { value: b64, bytes: 0, binary: true, error: "not valid base64" };
  }
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return { value: text, bytes: bytes.length, binary: false };
  } catch {
    return { value: b64, bytes: bytes.length, binary: true };
  }
}

/** Seed data for the Secret editor: the base64 `.data` split into editable
 *  plaintext (UTF-8) entries and a count of binary entries left untouched. */
export function decodeSecretData(
  manifest: string,
): { text: Record<string, string>; binaryCount: number; parseError: string | null } {
  let obj: unknown;
  try {
    obj = JSON.parse(manifest);
  } catch (e) {
    return { text: {}, binaryCount: 0, parseError: String(e) };
  }
  const data = (obj as { data?: Record<string, string> })?.data ?? {};
  const text: Record<string, string> = {};
  let binaryCount = 0;
  for (const [key, b64] of Object.entries(data)) {
    const d = decodeValue(b64);
    if (d.binary || d.error) binaryCount++;
    else text[key] = d.value;
  }
  return { text, binaryCount, parseError: null };
}

function decodeManifest(manifest: string): { entries: DecodedEntry[]; parseError: string | null } {
  let obj: unknown;
  try {
    obj = JSON.parse(manifest);
  } catch (e) {
    return { entries: [], parseError: String(e) };
  }
  const data = (obj as { data?: Record<string, string> })?.data ?? {};
  // `stringData` is normally absorbed into `data` server-side, but honor it if present.
  const stringData = (obj as { stringData?: Record<string, string> })?.stringData ?? {};

  const entries: DecodedEntry[] = [];
  for (const [key, b64] of Object.entries(data)) {
    const { value, bytes, binary, error } = decodeValue(b64);
    entries.push({ key, value, bytes, binary, error });
  }
  for (const [key, value] of Object.entries(stringData)) {
    entries.push({ key, value, bytes: new TextEncoder().encode(value).length, binary: false });
  }
  entries.sort((a, b) => a.key.localeCompare(b.key));
  return { entries, parseError: null };
}

/** Read-only viewer that base64-decodes a Secret's `data` to reveal plaintext.
 *  Values that aren't valid UTF-8 are shown as raw base64 and flagged binary. */
export function SecretDataViewer({ manifest }: Props) {
  const { entries, parseError } = useMemo(() => decodeManifest(manifest), [manifest]);

  if (parseError) {
    return <div className="dim" style={{ color: "var(--status-failed)", marginTop: 8 }}>{parseError}</div>;
  }
  if (entries.length === 0) {
    return <div className="dim" style={{ fontSize: "0.85em", marginTop: 8 }}>No data keys to decode.</div>;
  }

  return (
    <div className="secret-decoded">
      {entries.map((e) => (
        <div className="secret-entry" key={e.key}>
          <div className="secret-entry-head">
            <span className="secret-entry-key mono" title={e.key}>
              {e.key}
            </span>
            {e.error ? (
              <span className="secret-entry-tag err">{e.error}</span>
            ) : e.binary ? (
              <span className="secret-entry-tag" title="Value is not valid UTF-8; showing raw base64">
                binary · {e.bytes} B
              </span>
            ) : null}
            <CopyButton text={e.value} title="Copy value" />
          </div>
          <pre className="secret-entry-value mono">{e.value}</pre>
        </div>
      ))}
    </div>
  );
}
