import { useState } from "react";

import { tome } from "../service";
import { ChatAnswer, isTauri } from "../types";

interface AskTomeProps {
  /** Article currently in the Reader. Used to bias retrieval toward
   *  questions about this page. (Currently informational only — the
   *  retrieval call uses the question directly. A future commit will
   *  factor this into the prompt.) */
  articleTitle?: string | null;
  /** Open an article when the user clicks a citation. */
  onOpenArticle: (title: string) => void;
}

/**
 * "Ask Tome" — RAG chat surface inside the Reader.
 *
 * Collapsed by default. Click to expand, type a question, hit Ask. The
 * model runs entirely locally; if the user hasn't downloaded it yet (or
 * the build doesn't have `chat-inference` enabled), the error message is
 * surfaced inline rather than crashed.
 */
export default function AskTome({ onOpenArticle }: AskTomeProps) {
  const [open, setOpen] = useState(false);
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState<ChatAnswer | null>(null);
  const [phase, setPhase] = useState<"idle" | "thinking" | "error">("idle");
  const [error, setError] = useState<string | null>(null);

  async function ask() {
    if (!question.trim() || !isTauri()) return;
    setPhase("thinking");
    setError(null);
    setAnswer(null);
    try {
      const r = await tome.askTome(question.trim());
      setAnswer(r);
      setPhase("idle");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        className="fixed bottom-6 right-6 z-20 px-4 py-2 rounded-full text-sm text-white shadow-lg hover:shadow-xl transition-shadow"
        style={{ backgroundColor: "var(--tome-accent)" }}
        title="Ask Tome a question about Wikipedia"
      >
        Ask Tome
      </button>
    );
  }

  return (
    <div
      className="fixed bottom-6 right-6 z-20 w-96 max-w-[90vw] max-h-[70vh] flex flex-col rounded-lg border border-tome-border bg-tome-surface shadow-2xl overflow-hidden"
    >
      <div className="px-3 py-2 border-b border-tome-border flex items-center justify-between"
           style={{
             backgroundColor: "color-mix(in srgb, var(--tome-surface-2) 80%, transparent)",
           }}>
        <span className="text-sm font-semibold">Ask Tome</span>
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="text-xs text-tome-muted hover:text-tome-text"
          aria-label="Close"
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-auto px-3 py-3 space-y-3">
        {answer && (
          <div className="text-sm space-y-2">
            <div className="whitespace-pre-wrap text-tome-text">{answer.answer}</div>
            {answer.citations.length > 0 && (
              <div className="pt-2 border-t border-tome-border">
                <div className="text-[10px] uppercase tracking-wide text-tome-muted mb-1">
                  Citations
                </div>
                <ul className="space-y-1">
                  {answer.citations.map((c) => (
                    <li key={c} className="text-xs">
                      <span className="font-mono text-tome-muted mr-2">[A{c}]</span>
                      {/* The citation index points into the retrieval set.
                          We don't currently have the title-by-index mapping
                          back here in the UI; clicking does nothing for v1.
                          Future: include the retrieval set in ChatAnswer. */}
                      <span className="text-tome-text">source #{c}</span>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
        {phase === "thinking" && (
          <div className="text-sm text-tome-muted italic">
            Thinking… (may take a few seconds on first call as the model
            warms up)
          </div>
        )}
        {phase === "error" && error && (
          <div className="text-sm text-tome-danger whitespace-pre-wrap">
            {error}
            <div className="text-xs text-tome-muted mt-2">
              If this says &quot;chat inference disabled,&quot; the binary
              you&apos;re running was built without the inference backend.
              See Settings for the model download.
            </div>
          </div>
        )}
        {phase === "idle" && !answer && (
          <div className="text-xs text-tome-muted">
            Local LLM answers grounded in your downloaded Wikipedia
            articles. Citations point back to the sources used.
          </div>
        )}
      </div>

      <div className="px-3 py-2 border-t border-tome-border">
        <textarea
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              ask();
            }
          }}
          placeholder="Ask anything…  (⌘/Ctrl+Enter to send)"
          rows={2}
          disabled={phase === "thinking"}
          className="w-full px-2 py-1 text-sm rounded border border-tome-border bg-tome-bg resize-none disabled:opacity-50"
        />
        <div className="flex justify-between items-center pt-2">
          <span className="text-[10px] text-tome-muted">offline · local model</span>
          <button
            type="button"
            onClick={ask}
            disabled={phase === "thinking" || !question.trim() || !isTauri()}
            className="px-3 py-1 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
            style={{ backgroundColor: "var(--tome-accent)" }}
          >
            {phase === "thinking" ? "Thinking…" : "Ask"}
          </button>
        </div>
      </div>
      {/* Reserved: onOpenArticle wires up once we propagate retrieval-set
          titles back through ChatAnswer. */}
      {void onOpenArticle}
    </div>
  );
}
