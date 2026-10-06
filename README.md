> [!IMPORTANT]
> Remove this line to confirm you've reviewed this PR before submitting.

# Zed (Repack)

This repository is a **repack** of [Zed](https://zed.dev) maintained by [Belizário Ribeiro](https://github.com/belizariogr), with personalizations and additional features on top of the official upstream ([zed-industries/zed](https://github.com/zed-industries/zed)).

Zed is a high-performance, multiplayer code editor from the creators of [Atom](https://github.com/atom/atom) and [Tree-sitter](https://github.com/tree-sitter/tree-sitter).

---

## Features in this repack

Customizations implemented in this fork:

### Debugger

- **Bun debugger** — Bun DAP adapter/inspector, Bun debug locator, and bringing session windows forward on breakpoints
- **Watches without an active session** — add-watch controls always enabled; Variables visible before starting debug; watches retained in the project's DAP store across sessions and evaluated when a stack frame becomes available
- **Watch expression UI** — inline add-watch input in Variables; watches kept when evaluation fails or returns `undefined`
- **Add-watch + button** — create watches from the variables list without selecting an existing variable first; control lives beside the Console/Variables tabs
- **Stack frame indicator in the gutter** — dedicated gutter icon for the current debug line; VS Code-style outlined stack frame arrow drawn above (and around) a smaller breakpoint circle without blocking clicks
- **Browser debugger lifecycle** — disconnect ended/failed sessions without terminating the debuggee; reconnectable Chromium TCP launches; apply `launch.json` OS overrides before converting configs
- **Navigate after page debugger setup** — open the URL from `launch.json` after binding and configuring the browser page's child session, avoiding a cached service worker deadlock
- **Close debugger-launched browsers when their Zed window closes** — track the original browser endpoint until its owning Zed window closes or the app quits, preserving browsers owned by other Zed windows

### Editor / Languages

- **PascalCase as classes in JS/TS** — PascalCase identifiers highlighted as classes in JS/TS/TSX grammars and language config (same idea as Python class naming)

### Project Panel

- **Root state and collapse-all** — `is_root` handling with root-specific padding/collapse-all behavior; show the collapse-all button only while hovering the project row

### Installation

- **`script/install-macos`** — builds the fork in release mode as stable by default, preserves macOS bundle icons and document associations, and installs the app and CLI
- **`script/install-linux-system`** — system-wide Linux install (CLI in `/usr/bin`, editor under `/usr/lib/zed`), stable channel by default, and absolute desktop icon path

---

### Installation (upstream)

On macOS, Linux, and Windows you can [download Zed directly](https://zed.dev/download) or install Zed via your local package manager ([macOS](https://zed.dev/docs/installation#macos)/[Linux](https://zed.dev/docs/linux#installing-via-a-package-manager)/[Windows](https://zed.dev/docs/windows#package-managers)).

For this fork, prefer the scripts above to install the customized build.

Other platforms are not yet available:

- Web ([tracking discussion](https://github.com/zed-industries/zed/discussions/26195))

### Developing Zed

- [Building Zed for macOS](./docs/src/development/macos.md)
- [Building Zed for Linux](./docs/src/development/linux.md)
- [Building Zed for Windows](./docs/src/development/windows.md)

### Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md) for ways you can contribute to Zed upstream.

### Licensing

Zed source code is licensed primarily under GPL-3.0-or-later, with Apache-2.0 components where marked.

License information for third party dependencies must be correctly provided for CI to pass.

We use [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) to automatically comply with open source licenses. If CI is failing, check the following:

- Is it showing a `no license specified` error for a crate you've created? If so, add `publish = false` under `[package]` in your Cargo.toml.
- Is the error `failed to satisfy license requirements` for a dependency? If so, first determine what license the project has and whether this system is sufficient to comply with this license's requirements. If you're unsure, ask a lawyer. Once you've verified that this system is acceptable add the license's SPDX identifier to the `accepted` array in `script/licenses/zed-licenses.toml`.
- Is `cargo-about` unable to find the license for a dependency? If so, add a clarification field at the end of `script/licenses/zed-licenses.toml`, as specified in the [cargo-about book](https://embarkstudios.github.io/cargo-about/cli/generate/config.html#crate-configuration).

## Sponsorship

Zed is developed by **Zed Industries, Inc.**, a for-profit company.

If you’d like to financially support the project, you can do so via GitHub Sponsors.
Sponsorships go directly to Zed Industries and are used as general company revenue.
There are no perks or entitlements associated with sponsorship.
