# Tome Distribution Plan: Tauri 2 Packaging Research (April 2026)

## 1. Code Signing

**Windows.** The 2023 CA/B Forum rules still hold: OV/EV certs must live on an HSM. Three realistic options for indies:
- **Microsoft Trusted Signing** (rebranded "Azure Artifact Signing" in late 2025) — $9.99/mo, now open to individual developers in US/Canada (EU/UK orgs only). Cert is short-lived, fully managed, no hardware token. **This is the new default for indies.**
- **OV cert via Sectigo/SSL.com on Azure Key Vault** — ~$200–300/yr + ~$1/mo Key Vault. Still triggers SmartScreen until reputation builds (weeks to months of installs).
- **EV cert** — $400–600/yr, instant SmartScreen reputation, but requires a registered business entity. Overkill for a GPL-3.0 indie tool.

Unsigned ships fine technically, but every Windows user sees the blue "Windows protected your PC" SmartScreen wall and must click "More info -> Run anyway." Conversion drops noticeably.

**macOS.** Apple Developer Program: **$99/yr**, includes Developer ID cert + unlimited notarization. Non-negotiable if you ship a `.dmg`. Without it on Sonoma users could right-click -> Open; on **Sequoia (15)** that bypass is gone — they must dig into System Settings -> Privacy & Security after each launch attempt; on macOS 16 the friction is the same or worse. Effectively unsigned = unusable for non-technical Mac users in 2026.

**Linux.** No signing infrastructure to speak of. GPG-sign your release artifacts and publish the public key; Flathub handles its own signing on the repo side.

**Note on v1 -> v2 drift:** Tauri v1's docs leaned on local `signtool.exe` workflows. v2 docs (`v2.tauri.app/distribute/sign/windows/`) explicitly recommend HSM/Trusted Signing and document a `signCommand` hook for arbitrary signing tools — use the v2 path.

## 2. Auto-Updater

**Recommendation: official `@tauri-apps/plugin-updater` + static JSON on GitHub Releases.**

- It's first-party, uses Tauri's minisign-based signature verification (mandatory, not optional), and `tauri-action` already emits a `latest.json` shaped exactly like the plugin expects. Endpoint: `https://github.com/<user>/<repo>/releases/latest/download/latest.json`.
- **Velopack** is genuinely faster (delta updates, ~2s relaunch, no UAC) and more polished than Squirrel ever was, but it's a parallel toolchain to maintain alongside Tauri's bundler. Worth it for a commercial app pushing weekly updates; not worth it for Tome.
- The "Tauri Bundle Updater" hosted service you may have seen referenced is **CrabNebula Cloud** (the company behind much of Tauri 2 dev). Paid SaaS; skip unless you need analytics and rollout control.

## 3. Linux

In 2026 the consensus has firmed up:
- **Flatpak via Flathub** — primary channel. Largest install base, sandboxed, auto-updates, distro-agnostic. Submit here.
- **AppImage** — keep it as the "I just want to try it" download. Tauri builds it by default; ~70 MB with the WebKit bundle. Useful, low effort.
- **`.deb`** — cheap to produce (Tauri default), worth shipping.
- **Snap** — skip. Canonical-only, declining mindshare, and Tauri's snap support is rougher.
- **`.rpm`** — ship if effortless, otherwise skip; Fedora users can use the Flatpak.
- **AUR** — let a community packager handle it; don't self-maintain.

## 4. macOS Notarization Walkthrough

1. Enroll in Apple Developer Program ($99).
2. Create a Developer ID Application cert in your account; download to keychain.
3. Set `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific password), `APPLE_TEAM_ID` env vars.
4. `tauri build` — Tauri 2 invokes `codesign` then submits to `notarytool` automatically and staples the ticket to the `.dmg`.

Without notarization: Sonoma allows right-click -> Open workaround; Sequoia and macOS 16 force users into System Settings each launch. Don't ship unnotarized to Mac.

## 5. Hosting

**GitHub Releases.** No documented bandwidth cap on release assets (only LFS has the 1 GB/mo limit — different system). For a 30 MB binary at indie scale (tens of thousands of downloads/mo) it's free and reliable. If you ever go viral, front it with Cloudflare R2 ($0.015/GB egress, no egress fees to Cloudflare network) or B2. Don't preemptively over-engineer.

## 6. Reproducibility / Supply Chain

Rust still isn't byte-reproducible by default (Tauri's own security docs admit this). Practical mitigations: build releases in GitHub Actions from a tagged commit, publish SHA-256 sums + minisign signatures alongside artifacts, run `cargo-auditable` to embed an SBOM, and consider `cargo-vet` for dep review. Flathub now runs `flathub-repro-checker` — another reason to ship there.

## Recommended path for Tome

Year 1 budget ~$220: Apple Developer ($99) + Microsoft Trusted Signing ($120). Distribute via **GitHub Releases** (`.dmg` notarized, `.msi` Trusted-Signed, `.AppImage`, `.deb`) + **Flathub**. Use the **official Tauri updater plugin** with `latest.json` on GitHub Releases. Publish SHA-256 + minisign sigs. Skip Snap, skip EV cert, skip CrabNebula Cloud, skip Velopack.

Sources:
- [Tauri Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)
- [Tauri macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri Updater Plugin](https://v2.tauri.app/plugin/updater/)
- [Tauri AppImage docs](https://v2.tauri.app/distribute/appimage/)
- [Tauri Flathub docs](https://tauri.app/distribute/flatpak/)
- [Tauri Application Lifecycle Threats](https://v2.tauri.app/security/lifecycle/)
- [Trusted Signing for individual developers](https://techcommunity.microsoft.com/blog/microsoft-security-blog/trusted-signing-is-now-open-for-individual-developers-to-sign-up-in-public-previ/4273554)
- [Azure Artifact Signing pricing](https://azure.microsoft.com/en-us/pricing/details/artifact-signing/)
- [Apple Developer Program enrollment](https://developer.apple.com/programs/)
- [Apple Notarization docs](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [macOS Sequoia Gatekeeper change (MacRumors)](https://www.macrumors.com/2024/08/06/macos-sequoia-gatekeeper-security-change/)
- [Sequoia Gatekeeper analysis (Eclectic Light)](https://eclecticlight.co/2024/08/10/gatekeeper-and-notarization-in-sequoia/)
- [Best Code Signing Cert Providers 2026](https://sslinsights.com/best-code-signing-certificate-providers/)
- [Velopack](https://velopack.io/)
- [CrabNebula auto-updates guide](https://docs.crabnebula.dev/cloud/guides/auto-updates-tauri/)
- [Tauri updater + GitHub Releases automation](https://github.com/tauri-apps/tauri/discussions/10206)
- [Snap vs Flatpak vs AppImage 2026](https://oneuptime.com/blog/post/2026-03-02-how-to-choose-between-snap-flatpak-and-appimage-on-ubuntu/view)
- [Reproducible Builds March 2026](https://reproducible-builds.org/reports/2026-03/)
- [Shipping production Tauri 2 macOS guide](https://dev.to/0xmassi/shipping-a-production-macos-app-with-tauri-20-code-signing-notarization-and-homebrew-mc3)