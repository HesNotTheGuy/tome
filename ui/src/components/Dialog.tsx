import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";

/**
 * Themed, in-app replacements for the browser's `confirm()` and `prompt()`.
 *
 * Native dialogs are unstyled OS chrome that break the illusion of a polished
 * desktop app. These match the Tome theme and are promise-based, so call sites
 * read almost exactly like the browser primitives they replace:
 *
 *   const confirm = useConfirm();
 *   if (await confirm({ message: "Delete this?", danger: true })) { … }
 *
 *   const prompt = usePrompt();
 *   const name = await prompt({ title: "Folder name", defaultValue: "" });
 *   if (name !== null) { … }
 *
 * Wrap the app once in <DialogProvider> (done in App.tsx); the hooks throw if
 * used outside it, which surfaces wiring mistakes immediately.
 */

interface ConfirmOptions {
  title?: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Style the confirm button as destructive (used for delete/replace). */
  danger?: boolean;
}

interface PromptOptions {
  title?: string;
  message?: string;
  defaultValue?: string;
  placeholder?: string;
  confirmLabel?: string;
}

type ConfirmFn = (opts: ConfirmOptions) => Promise<boolean>;
type PromptFn = (opts: PromptOptions) => Promise<string | null>;

interface DialogApi {
  confirm: ConfirmFn;
  prompt: PromptFn;
}

const DialogContext = createContext<DialogApi | null>(null);

export function useConfirm(): ConfirmFn {
  const ctx = useContext(DialogContext);
  if (!ctx) throw new Error("useConfirm must be used within <DialogProvider>");
  return ctx.confirm;
}

export function usePrompt(): PromptFn {
  const ctx = useContext(DialogContext);
  if (!ctx) throw new Error("usePrompt must be used within <DialogProvider>");
  return ctx.prompt;
}

// Internal union describing the dialog currently on screen (if any).
type ActiveDialog =
  | {
      kind: "confirm";
      opts: ConfirmOptions;
      resolve: (v: boolean) => void;
    }
  | {
      kind: "prompt";
      opts: PromptOptions;
      resolve: (v: string | null) => void;
    };

export function DialogProvider({ children }: { children: React.ReactNode }) {
  const [active, setActive] = useState<ActiveDialog | null>(null);
  const [inputValue, setInputValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  const confirm = useCallback<ConfirmFn>((opts) => {
    return new Promise<boolean>((resolve) => {
      setActive({ kind: "confirm", opts, resolve });
    });
  }, []);

  const prompt = useCallback<PromptFn>((opts) => {
    setInputValue(opts.defaultValue ?? "");
    return new Promise<string | null>((resolve) => {
      setActive({ kind: "prompt", opts, resolve });
    });
  }, []);

  // Focus the input / confirm button when a dialog opens.
  useEffect(() => {
    if (active?.kind === "prompt") {
      // Defer so the element exists.
      const id = setTimeout(() => {
        inputRef.current?.focus();
        inputRef.current?.select();
      }, 0);
      return () => clearTimeout(id);
    }
  }, [active]);

  function close(result: boolean | string | null) {
    if (!active) return;
    if (active.kind === "confirm") {
      active.resolve(result as boolean);
    } else {
      active.resolve(result as string | null);
    }
    setActive(null);
  }

  // Escape cancels, Enter confirms.
  useEffect(() => {
    if (!active) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        close(active!.kind === "confirm" ? false : null);
      } else if (e.key === "Enter" && active!.kind === "prompt") {
        e.preventDefault();
        close(inputValue);
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, inputValue]);

  return (
    <DialogContext.Provider value={{ confirm, prompt }}>
      {children}
      {active && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ backgroundColor: "color-mix(in srgb, black 45%, transparent)" }}
          onMouseDown={(e) => {
            // Click on the backdrop cancels.
            if (e.target === e.currentTarget) {
              close(active.kind === "confirm" ? false : null);
            }
          }}
        >
          <div
            role="dialog"
            aria-modal="true"
            className="w-full max-w-sm rounded-lg border border-tome-border bg-tome-surface shadow-xl p-4"
          >
            {active.opts.title && (
              <h2 className="text-sm font-bold text-tome-text mb-2">
                {active.opts.title}
              </h2>
            )}
            {active.kind === "confirm" && (
              <p className="text-sm text-tome-text whitespace-pre-line">
                {active.opts.message}
              </p>
            )}
            {active.kind === "prompt" && (
              <>
                {active.opts.message && (
                  <p className="text-sm text-tome-text mb-2">
                    {active.opts.message}
                  </p>
                )}
                <input
                  ref={inputRef}
                  type="text"
                  value={inputValue}
                  onChange={(e) => setInputValue(e.target.value)}
                  placeholder={active.opts.placeholder}
                  className="w-full px-2 py-1 text-sm rounded border border-tome-border bg-tome-bg focus:outline-none focus:ring-2 focus:ring-blue-500"
                />
              </>
            )}

            <div className="mt-4 flex items-center justify-end gap-2">
              <button
                type="button"
                onClick={() => close(active.kind === "confirm" ? false : null)}
                className="px-3 py-1 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
              >
                {active.kind === "confirm"
                  ? (active.opts.cancelLabel ?? "Cancel")
                  : "Cancel"}
              </button>
              <button
                type="button"
                onClick={() =>
                  close(active.kind === "confirm" ? true : inputValue)
                }
                className="px-3 py-1 text-sm rounded text-white"
                style={{
                  backgroundColor:
                    active.kind === "confirm" && active.opts.danger
                      ? "var(--tome-danger)"
                      : "var(--tome-accent)",
                }}
              >
                {active.kind === "confirm"
                  ? (active.opts.confirmLabel ?? "Confirm")
                  : (active.opts.confirmLabel ?? "OK")}
              </button>
            </div>
          </div>
        </div>
      )}
    </DialogContext.Provider>
  );
}
