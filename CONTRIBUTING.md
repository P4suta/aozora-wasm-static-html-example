# Contributing

Use the Bun version in `package.json` and the Rust toolchain in `rust-toolchain.toml`.
Install the Typos version from `.github/workflows/ci.yml` and the Chromium, Firefox, and WebKit Playwright browsers.
Then run:

```sh
bun install --frozen-lockfile
bun run ci
```

| Command                                                    | Purpose                                                 |
| ---------------------------------------------------------- | ------------------------------------------------------- |
| `bun run check`                                            | Check source, tests, and interfaces.                    |
| `bun run lab`                                              | Run the default deterministic WASM check.               |
| `bun run lab doctor --engine all`                          | Check host tools, workers, artifacts, and hashes.       |
| `bun run lab verify --engine wasm --scope quick`           | Verify a minimal feature-covering corpus.               |
| `bun run lab verify --engine all --scope full --shard 0/4` | Verify one full-corpus shard across every distribution. |
| `bun run lab build --engine wasm`                          | Verify and build the static site.                       |
| `bun run lab visual --baseline PATH --candidate PATH`      | Compare browser output and layout.                      |
| `bun run lab full --baseline PATH --corpus PATH`           | Run the complete rights-filtered corpus gate.           |
| `bun run spellcheck`                                       | Check spelling with Typos.                              |

The versioned interfaces are [the rights corpus manifest](docs/corpus-manifest-v1.md) and [the worker protocol](docs/worker-protocol-v1.md).

This repository owns the verifier, release orchestration, corpus and worker contracts, visual comparison, static-page composition, and consumer UI.
Parser behavior and distribution workers belong in `P4suta/aozora`.
Do not edit or commit `dist/`.

Never update diagnostics or visual baselines automatically.
Baseline changes require review of the complete release result.
The full stable-channel workflow runs on demand, while upstream release candidates call the reusable verification gate.
See [the verification decision](docs/adr/stable-channel-verification.md).
