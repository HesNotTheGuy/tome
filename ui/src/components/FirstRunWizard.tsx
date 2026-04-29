import { useState } from "react";

import { tome } from "../service";
import { isTauri } from "../types";

interface FirstRunWizardProps {
  onComplete: () => void;
  onSkip: () => void;
}

type Step = "welcome" | "download" | "configure" | "ingesting" | "done";

/**
 * First-run guided setup. Shown when the user has no dump configured and
 * no articles in storage. Walks them through:
 *
 * 1. What Tome needs from them.
 * 2. Where to download a Wikipedia dump (we don't auto-download yet — the
 *    user's browser handles the 1-30 GB transfer with proper resume support).
 * 3. Pasting the resulting paths.
 * 4. Watching the ingest run.
 *
 * After ingest succeeds, the wizard dismisses and the user lands in Library
 * with their Cold tier populated. They can re-open it any time from
 * Settings → "Run setup again."
 */
export default function FirstRunWizard({ onComplete, onSkip }: FirstRunWizardProps) {
  const [step, setStep] = useState<Step>("welcome");
  const [dumpPath, setDumpPath] = useState("");
  const [indexPath, setIndexPath] = useState("");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState<{ count: number; ms: number } | null>(null);

  async function runIngest() {
    if (!isTauri()) {
      setError("This step requires the Tauri shell.");
      return;
    }
    if (!dumpPath.trim() || !indexPath.trim()) {
      setError("Both paths are required.");
      return;
    }
    setError(null);
    setProgress(0);
    setStep("ingesting");
    try {
      // Persist the dump path first so Cold reads work after ingest.
      await tome.setDumpPath(dumpPath.trim());
      const result = await tome.ingestIndex(indexPath.trim(), (n) => setProgress(n));
      setSummary({ count: result.entries_processed, ms: result.elapsed_ms });
      setStep("done");
    } catch (e) {
      setError(String(e));
      setStep("configure");
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
      <div className="w-full max-w-2xl mx-4 rounded-lg border border-tome-border bg-tome-surface shadow-2xl overflow-hidden">
        <div
          className="px-6 py-3 border-b border-tome-border flex items-center justify-between"
          style={{
            backgroundColor: "color-mix(in srgb, var(--tome-surface-2) 80%, transparent)",
          }}
        >
          <div className="flex items-center gap-3">
            <span className="font-bold text-lg">Welcome to Tome</span>
            <span className="text-xs text-tome-muted">
              {step === "welcome" && "1 of 3"}
              {step === "download" && "2 of 3"}
              {step === "configure" && "3 of 3"}
              {step === "ingesting" && "Loading…"}
              {step === "done" && "Ready"}
            </span>
          </div>
          <button
            type="button"
            onClick={onSkip}
            className="text-xs text-tome-muted hover:text-tome-text"
          >
            Skip setup
          </button>
        </div>

        <div className="px-6 py-5">
          {step === "welcome" && <WelcomeStep onNext={() => setStep("download")} />}
          {step === "download" && (
            <DownloadStep
              onBack={() => setStep("welcome")}
              onNext={() => setStep("configure")}
            />
          )}
          {step === "configure" && (
            <ConfigureStep
              dumpPath={dumpPath}
              indexPath={indexPath}
              setDumpPath={setDumpPath}
              setIndexPath={setIndexPath}
              error={error}
              onBack={() => setStep("download")}
              onIngest={runIngest}
            />
          )}
          {step === "ingesting" && <IngestingStep progress={progress} />}
          {step === "done" && summary && (
            <DoneStep summary={summary} onFinish={onComplete} />
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <div className="space-y-4">
      <p className="text-tome-text">
        Tome reads Wikipedia offline from a copy you keep on disk. To get
        started, you&apos;ll need two files from Wikipedia&apos;s official
        dumps. Tome doesn&apos;t modify them — it just seeks into them when you
        open an article.
      </p>
      <div className="rounded border border-tome-border bg-tome-surface-2 p-4 text-sm space-y-2">
        <div className="font-semibold">What you&apos;ll do:</div>
        <ol className="list-decimal pl-5 space-y-1 text-tome-text">
          <li>Download two files from dumps.wikimedia.org (we&apos;ll show you which).</li>
          <li>Tell Tome where you saved them.</li>
          <li>Wait for the index to load (~1-5 minutes for Simple English).</li>
        </ol>
      </div>
      <p className="text-xs text-tome-muted">
        Already have a dump? You can paste the paths in step 3 and skip the
        download.
      </p>
      <div className="flex justify-end pt-2">
        <button
          type="button"
          onClick={onNext}
          className="px-4 py-2 text-sm rounded text-white"
          style={{ backgroundColor: "var(--tome-accent)" }}
        >
          Let&apos;s go →
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function DownloadStep({
  onBack,
  onNext,
}: {
  onBack: () => void;
  onNext: () => void;
}) {
  return (
    <div className="space-y-4">
      <p className="text-sm text-tome-text">
        Pick whichever wiki you want. Both come as a pair: the dump itself,
        and a small index file. Save them to the same folder.
      </p>

      <div className="grid gap-3 sm:grid-cols-2">
        <DumpCard
          title="Simple English"
          size="~1 GB"
          articles="~250,000"
          recommended
          base="https://dumps.wikimedia.org/simplewiki/latest/"
          dump="simplewiki-latest-pages-articles-multistream.xml.bz2"
          index="simplewiki-latest-pages-articles-multistream-index.txt.bz2"
        />
        <DumpCard
          title="English (full)"
          size="~24 GB"
          articles="~6.8M"
          base="https://dumps.wikimedia.org/enwiki/latest/"
          dump="enwiki-latest-pages-articles-multistream.xml.bz2"
          index="enwiki-latest-pages-articles-multistream-index.txt.bz2"
        />
      </div>

      <div className="rounded border border-tome-border bg-tome-surface-2 p-3 text-xs text-tome-muted">
        <div className="font-semibold text-tome-text mb-1">Tip</div>
        Big downloads are best done in your browser — it handles
        pause/resume. Tome will read whatever you end up with.
      </div>

      <div className="flex justify-between pt-2">
        <button
          type="button"
          onClick={onBack}
          className="px-3 py-1.5 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
        >
          ← Back
        </button>
        <button
          type="button"
          onClick={onNext}
          className="px-4 py-2 text-sm rounded text-white"
          style={{ backgroundColor: "var(--tome-accent)" }}
        >
          I&apos;ve downloaded them →
        </button>
      </div>
    </div>
  );
}

function DumpCard({
  title,
  size,
  articles,
  recommended,
  base,
  dump,
  index,
}: {
  title: string;
  size: string;
  articles: string;
  recommended?: boolean;
  base: string;
  dump: string;
  index: string;
}) {
  return (
    <div className="rounded border border-tome-border bg-tome-surface p-4 space-y-2 relative">
      {recommended && (
        <span
          className="absolute top-2 right-2 px-2 py-0.5 rounded text-[10px] font-semibold uppercase text-white"
          style={{ backgroundColor: "var(--tome-success)" }}
        >
          Start here
        </span>
      )}
      <div className="font-semibold">{title}</div>
      <div className="text-xs text-tome-muted">
        {size} · {articles} articles
      </div>
      <div className="space-y-1 pt-1">
        <FileLink label="Dump" url={base + dump} filename={dump} />
        <FileLink label="Index" url={base + index} filename={index} />
      </div>
    </div>
  );
}

function FileLink({
  label,
  url,
  filename,
}: {
  label: string;
  url: string;
  filename: string;
}) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="font-semibold text-tome-muted w-12 shrink-0">{label}:</span>
      <a
        href={url}
        className="font-mono truncate text-tome-link hover:underline"
        title={filename}
      >
        {filename}
      </a>
    </div>
  );
}

// ---------------------------------------------------------------------------

function ConfigureStep({
  dumpPath,
  indexPath,
  setDumpPath,
  setIndexPath,
  error,
  onBack,
  onIngest,
}: {
  dumpPath: string;
  indexPath: string;
  setDumpPath: (s: string) => void;
  setIndexPath: (s: string) => void;
  error: string | null;
  onBack: () => void;
  onIngest: () => void;
}) {
  return (
    <div className="space-y-4">
      <p className="text-sm text-tome-text">
        Paste the full paths to the two files you downloaded. Tome will index
        the smaller of the two (the index file) and remember where the dump
        lives for later reads.
      </p>

      <div className="space-y-3">
        <div>
          <label className="block text-xs font-semibold uppercase tracking-wide text-tome-muted mb-1">
            Dump file (*.xml.bz2)
          </label>
          <input
            type="text"
            value={dumpPath}
            onChange={(e) => setDumpPath(e.target.value)}
            placeholder="C:\path\to\simplewiki-latest-pages-articles-multistream.xml.bz2"
            className="w-full px-3 py-2 text-xs font-mono rounded border border-tome-border bg-tome-bg"
          />
        </div>
        <div>
          <label className="block text-xs font-semibold uppercase tracking-wide text-tome-muted mb-1">
            Index file (*-index.txt.bz2)
          </label>
          <input
            type="text"
            value={indexPath}
            onChange={(e) => setIndexPath(e.target.value)}
            placeholder="C:\path\to\simplewiki-latest-pages-articles-multistream-index.txt.bz2"
            className="w-full px-3 py-2 text-xs font-mono rounded border border-tome-border bg-tome-bg"
          />
        </div>
      </div>

      {error && (
        <div className="rounded border border-tome-danger/50 bg-tome-danger/10 p-3 text-sm text-tome-danger">
          {error}
        </div>
      )}

      <div className="flex justify-between pt-2">
        <button
          type="button"
          onClick={onBack}
          className="px-3 py-1.5 text-sm rounded border border-tome-border text-tome-muted hover:bg-tome-surface-2"
        >
          ← Back
        </button>
        <button
          type="button"
          onClick={onIngest}
          disabled={!dumpPath.trim() || !indexPath.trim()}
          className="px-4 py-2 text-sm rounded text-white disabled:opacity-50 disabled:cursor-not-allowed"
          style={{ backgroundColor: "var(--tome-accent)" }}
        >
          Index it →
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function IngestingStep({ progress }: { progress: number }) {
  return (
    <div className="py-6 space-y-4 text-center">
      <div className="text-tome-text">
        Loading article index. This is a one-time step.
      </div>
      <div className="text-3xl font-mono">{progress.toLocaleString()}</div>
      <div className="text-xs text-tome-muted">
        articles indexed so far · do not close the window
      </div>
      <div className="h-1 bg-tome-surface-2 rounded overflow-hidden">
        <div
          className="h-full transition-all"
          style={{
            width: `${Math.min((progress / 250_000) * 100, 99)}%`,
            backgroundColor: "var(--tome-accent)",
          }}
        />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------

function DoneStep({
  summary,
  onFinish,
}: {
  summary: { count: number; ms: number };
  onFinish: () => void;
}) {
  return (
    <div className="space-y-4 text-center py-4">
      <div className="text-4xl">✓</div>
      <div className="text-lg font-semibold">
        {summary.count.toLocaleString()} articles indexed
      </div>
      <div className="text-xs text-tome-muted">
        in {(summary.ms / 1000).toFixed(1)}s · ready to search
      </div>
      <div className="text-sm text-tome-text pt-2">
        You can now search any article from the bar at the top, or browse by
        category. Optional next steps live in Settings: ingest the
        categorylinks, geotag, or redirect tables for richer features.
      </div>
      <div className="pt-2">
        <button
          type="button"
          onClick={onFinish}
          className="px-4 py-2 text-sm rounded text-white"
          style={{ backgroundColor: "var(--tome-accent)" }}
        >
          Open Tome →
        </button>
      </div>
    </div>
  );
}
