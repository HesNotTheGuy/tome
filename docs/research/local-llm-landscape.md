# Local LLM for Tome's "Ask Tome" RAG Feature — April 2026

## 1. Model Recommendations

For a 4-8 GB RAM budget with citation-grounded RAG over Wikipedia, the realistic shortlist is **Qwen 3 4B**, **Gemma 3 4B IT**, **Phi-4-mini**, and **Llama 3.2 3B**. Larger 7-9B variants exist (Qwen 3 8B, Llama 3.3, Phi-4 14B) but push past laptop comfort levels.

| Model | 4-bit GGUF size | Inference RAM | M-series tok/s (Q4_K_M) | x86 CPU tok/s | License |
|---|---|---|---|---|---|
| Qwen 3 4B | ~2.4 GB | ~4-5 GB | 60-90 | 8-15 | Apache 2.0 |
| Gemma 3 4B IT | ~2.5 GB | ~4-5 GB | 70-100 | 10-15 | Gemma TOU (Gemma 4 moved to Apache 2.0) |
| Phi-4-mini (3.8B) | ~2.3 GB | ~4 GB | 80-120 | 10-18 | MIT |
| Llama 3.2 3B | ~2.0 GB | ~3.5 GB | 90-130 | 12-20 | Llama Community License |

**Licensing for a GPL-3.0 app distributed to end users:** Model weights are not "linked code" in the FSF sense, but you are still bound by the model's distribution terms.
- **Cleanest fits:** Phi-4-mini (MIT) and Qwen 3 (Apache 2.0). Either can be redistributed with the app or downloaded on first run with no friction.
- **Llama 3.x:** Permissive enough for commercial use, but the Community License imposes a "Built with Llama" attribution requirement and a 700M MAU cutover. Compatible in practice; GPL purists will flag the attribution clause as a non-free additional restriction.
- **Gemma 3:** Custom Gemma Terms of Use with acceptable-use restrictions. Gemma 4 has reportedly switched to Apache 2.0; if you can use Gemma 4 by ship date it becomes the strongest pick. Verify before bundling.

**Recommendation:** Default to **Qwen 3 4B Instruct** for instruction following + multilingual Wikipedia coverage, with **Phi-4-mini** as a smaller/faster alternative. Both have clean licenses, both follow citation-style prompts well, both support GBNF grammar-constrained decoding via llama.cpp.

## 2. Rust Integration

**`llama-cpp-2`** (recommended): Direct FFI to llama.cpp via bindgen. Tracks upstream closely, gets new quantization formats and model architectures within days. Build complexity is the main pain — you need a C++ toolchain, and Windows MSVC + CUDA combinations are fiddly. Binary impact ~5-15 MB linked statically. Cross-platform support is excellent (Win/macOS/Linux, Metal, CUDA, Vulkan). Runtime stability is high once you're past the build. The unsafe API surface is real but well-trodden.

**`candle`**: Pure Rust, trivial build, smaller binary, no C++ toolchain needed. The honest tradeoff: it lags llama.cpp on quantized CPU inference (typically 30-50% slower on Q4_K_M) and on supporting brand-new model variants. For a CPU-bound desktop app over a 4B model, that's the difference between "snappy" and "noticeably waiting." Use Candle if build simplicity outweighs performance.

**Sidecar (llama-server / Ollama)**: Easiest to ship initially, worst end-user experience. Ollama adds ~150 MB and a separate service users may already have installed (conflict risk on ports/models). `llama-server` as a bundled sidecar is cleaner — ship the binary in your installer, spawn it on launch, talk over loopback HTTP. Tauri's sidecar mechanism handles this well. Downside: process lifecycle management, slower cold start, harder to ship a clean uninstall.

**Recommendation:** **`llama-cpp-2` for production**, with the build pain front-loaded once. If you want to ship something this quarter, prototype with bundled `llama-server` sidecar and migrate later.

## 3. Citation-Grounded RAG

Native citation handling is weak across all small models — none of these reliably cite without prompt scaffolding. What works in practice:

1. **Pre-tag every chunk with a stable ID** in the context: `[A1] <article title>: <chunk>`. Don't let the model invent IDs.
2. **Force structured output via GBNF grammar** (llama.cpp supports this natively). Constrain output to `{"answer": str, "citations": ["A1", "A3"]}` where the citation enum is the literal set of IDs you injected. This eliminates hallucinated IDs at the decoder level — a major advantage of going through llama.cpp rather than Candle.
3. **Post-validate**: drop any answer where cited chunks don't lexically overlap claims. The 2026 literature ("FACTUM", grounding research) confirms citation presence ≠ groundedness; mechanical verification is necessary.
4. **Prompt pattern**: "First quote the supporting passage, then answer, then list citation IDs." Chain-of-grounding reduces post-hoc rationalization.

Qwen 3 and Phi-4-mini both follow this pattern well. Llama 3.2 3B is weaker at strict ID adherence without grammar constraints.

## 4. Onboarding UX

**Recommended:** First-run download with non-blocking progress indicator. Reasoning: a 2-4 GB installer kills conversion rates and forces re-downloads on every update. A small (~50 MB) installer that fetches the model on first "Ask Tome" activation is the dominant pattern in 2026 desktop AI apps.

Specifics:
- Trigger download lazily on first "Ask Tome" use, not at install. Most Tome users won't touch the AI feature.
- Side-snackbar or status-bar progress, not a modal — let users keep reading articles.
- Resume support (HTTP Range), SHA256 verify on completion.
- Offer one default model; expose a "switch model" picker in settings for power users but don't lead with it.
- Mirror through a CDN you control if possible; HuggingFace direct can be flaky and rate-limited.

Bundling the model only makes sense if your audience is offline-first by definition (which Tome's is) — in that case, offer a "full offline installer" variant alongside a thin one. Both should exist.

## Uncertainties

- Gemma 3 vs Gemma 4 license status in April 2026 is contested in sources; verify before bundling.
- Candle vs llama.cpp performance gap numbers vary by hardware; the direction is consistent (llama.cpp faster on quantized CPU) but the magnitude isn't well-benchmarked head-to-head.
- "Llama Community License + GPL-3.0 app" is legally untested; conservative read is to avoid it.

Sources:
- [Best Small AI Models 2026 (Local AI Master)](https://localaimaster.com/blog/small-language-models-guide-2026)
- [Best Open-Source SLMs 2026 (BentoML)](https://www.bentoml.com/blog/the-best-open-source-small-language-models)
- [Open-Source AI Landscape April 2026](https://www.digitalapplied.com/blog/open-source-ai-landscape-april-2026-gemma-qwen-llama)
- [Qwen3 license (HuggingFace)](https://huggingface.co/Qwen/Qwen3-8B/blob/main/LICENSE)
- [Phi-4 license (HuggingFace)](https://huggingface.co/microsoft/phi-4/blob/main/LICENSE)
- [Llama 3.3 Community License](https://www.llama.com/llama3_3/license/)
- [Gemma 4 Apache 2.0 (VentureBeat)](https://venturebeat.com/technology/google-releases-gemma-4-under-apache-2-0-and-that-license-change-may-matter)
- [llama-cpp-2 crate](https://crates.io/crates/llama-cpp-2)
- [HuggingFace Candle](https://github.com/huggingface/candle)
- [Apple MLX vs llama.cpp vs Candle](https://medium.com/@zaiinn440/apple-mlx-vs-llama-cpp-vs-hugging-face-candle-rust-for-lightning-fast-llms-locally-5447f6e9255a)
- [llama.cpp vs MLX vs Ollama Apple Silicon 2026](https://contracollective.com/blog/llama-cpp-vs-mlx-ollama-vllm-apple-silicon-2026)
- [Tauri sidecar docs](https://v2.tauri.app/develop/sidecar/)
- [Tauri + sidecar pattern (Evil Martians)](https://evilmartians.com/chronicles/making-desktop-apps-with-revved-up-potential-rust-tauri-sidecar)
- [RAG Grounding: 11 Tests That Expose Fake Citations](https://medium.com/@Nexumo_/rag-grounding-11-tests-that-expose-fake-citations-30d84140831a)
- [FACTUM: Mechanistic Detection of Citation Hallucination](https://arxiv.org/pdf/2601.05866)
- [Constraining LLMs with Structured Output (Qwen3)](https://www.glukhov.org/post/2025/09/llm-structured-output-with-ollama-in-python-and-go/)
- [Apple Silicon LLM Benchmarks (llmcheck.net)](https://llmcheck.net/benchmarks)
- [Calmer onboarding + non-blocking model download (GitHub issue)](https://github.com/tinyhumansai/openhuman/issues/101)