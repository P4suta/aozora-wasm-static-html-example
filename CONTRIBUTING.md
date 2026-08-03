# Contributing

Install Bun 1.3.14, Rust 1.97.1, typos-cli 1.48.0, and the Chromium, Firefox, and WebKit Playwright
browsers. Then run:

```sh
bun install --frozen-lockfile
bun run ci
```

This repository owns the verifier, release orchestration, corpus and worker contracts, visual
comparison, static-page composition, and consumer UI. Parser behavior and distribution workers
belong in `P4suta/aozora`. Do not edit or commit `dist/`.

Never update diagnostics or visual baselines automatically. Baseline changes require review of the
complete release result.
