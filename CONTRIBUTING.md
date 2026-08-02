# Contributing

Requirements are Bun 1.3.14, Rust 1.97.1, and Chromium/Firefox/WebKit Playwright environments.

```sh
bun install --frozen-lockfile
bun run ci
```

Do not edit or commit `dist/`; CI and GitHub Pages regenerate it. Change the generator or an input
and run `bun run build` to verify the deterministic output. Run `bun run lab` for the default real-work
quick gate. A release input must use the versioned rights-filtered corpus contract; `works.json` is
development-only. Release orchestration belongs in `rust/xtask.rs`; keep workflow `run` steps to one
portable command and do not add shell-script control flow.

Keep parser behavior and distribution worker implementations in `P4suta/aozora`. This repository
owns the Rust verifier/orchestrator, corpus and worker contracts, visual comparison, static-page
composition, and the consumer presentation layer. Never update diagnostics or visual baselines
without review. The build emits a candidate diagnostics baseline, but it becomes authoritative only
after the complete release fan-in is approved.
