import { isTauri } from "../types";

/**
 * A file-path input with a native "Browse…" picker button.
 *
 * This is the ONE place the app talks to the dialog plugin — every pane that
 * needs a path (dump, index, SQL dumps, pmtiles, models, backups) renders a
 * PathField instead of a bare text input, so the pick-or-paste behavior and
 * styling stay consistent everywhere.
 *
 * The text input always works (paste/type), so nothing regresses for users
 * who prefer paths or run outside the Tauri shell — the Browse button simply
 * hides when no native dialog is available.
 */
interface PathFieldProps {
  value: string;
  onChange: (value: string) => void;
  /** "openFile" picks an existing file; "saveFile" picks a destination. */
  mode?: "openFile" | "saveFile";
  /** Native dialog filters, e.g. [{ name: "JSON", extensions: ["json"] }]. */
  filters?: { name: string; extensions: string[] }[];
  /** Suggested filename for saveFile mode. */
  defaultFileName?: string;
  placeholder?: string;
  disabled?: boolean;
}

export default function PathField({
  value,
  onChange,
  mode = "openFile",
  filters,
  defaultFileName,
  placeholder,
  disabled,
}: PathFieldProps) {
  async function browse() {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const picked =
        mode === "saveFile"
          ? await dialog.save({ filters, defaultPath: defaultFileName })
          : await dialog.open({ multiple: false, directory: false, filters });
      if (typeof picked === "string" && picked) onChange(picked);
    } catch {
      // Dialog unavailable (e.g. plugin missing in a dev build) — the text
      // input still works, so failing silently is the right degradation.
    }
  }

  return (
    <div className="flex gap-2">
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        placeholder={placeholder}
        className="flex-1 px-2 py-1 text-xs font-mono rounded border border-tome-border bg-tome-bg disabled:opacity-50"
      />
      {isTauri() && (
        <button
          type="button"
          onClick={browse}
          disabled={disabled}
          className="px-2 py-1 text-xs rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2 hover:text-tome-text disabled:opacity-50 shrink-0"
        >
          Browse…
        </button>
      )}
    </div>
  );
}
