# aozora distribution verification lab

This repository verifies that seven `aozora` distributions produce the same output for a pinned
Aozora Bunko corpus, then builds a static site without client-side JavaScript.

## Development

Install Bun 1.3.14, Rust 1.97.1, typos-cli 1.48.0, and the Playwright browsers.

```sh
bun install --frozen-lockfile
bun run check
```

| Command | Purpose |
| --- | --- |
| `bun run lab` | Run the default deterministic WASM check. |
| `bun run lab doctor --engine all` | Check host tools, workers, artifacts, and hashes. |
| `bun run lab verify --engine wasm --scope quick` | Verify a minimal feature-covering corpus. |
| `bun run lab verify --engine all --scope full --shard 0/4` | Verify one full-corpus shard across every distribution. |
| `bun run lab build --engine wasm` | Verify and build the static site. |
| `bun run lab visual --baseline PATH --candidate PATH` | Compare browser output and layout. |
| `bun run lab full --baseline PATH --corpus PATH` | Run the complete rights-filtered corpus gate. |
| `bun run spellcheck` | Check spelling with typos-cli. |
| `bun run ci` | Run all local quality and browser checks. |

The versioned interfaces are documented in:

- [Rights corpus manifest v1](docs/corpus-manifest-v1.md)
- [JSONL worker protocol v1](docs/worker-protocol-v1.md)
- [Release integration](docs/release-integration.md)

## Data and license

`works.json` and `sources/` are a development-only legacy corpus. Release commands require the
pinned manifest and UTF-8 files produced by
[`P4suta/aozora-rights-filtered-corpus`](https://github.com/P4suta/aozora-rights-filtered-corpus).

Texts and bibliographic data come from [Aozora Bunko](https://www.aozora.gr.jp/). This project is
not an official Aozora Bunko service. Its automated rights checks are conservative rules for
display in Japan and hosting in the United States, not a guarantee for every jurisdiction.

The verification and site-generation code, including the vendored `aozora` CSS, is available under
Apache-2.0 or MIT. Those licenses do not relicense the texts.
