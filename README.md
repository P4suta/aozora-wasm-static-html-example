# aozora-wasm-static-html-example

Static HTML reading pages built from Aozora Bunko text with aozora-wasm

[Read the published pages](https://p4suta.github.io/aozora-wasm-static-html-example/).
The generated site needs no client-side JavaScript.

## Build

Install the Bun version in `package.json` and the Rust toolchain in `rust-toolchain.toml`, then run:

```sh
bun install --frozen-lockfile
bun run build
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for checks and [release integration](docs/release-integration.md) for verification across seven distributions.

## Data and license

Texts and bibliographic data come from [Aozora Bunko](https://www.aozora.gr.jp/).
This project is not an official Aozora Bunko service.
`works.json` and `sources/` provide a development corpus.
Releases require the pinned [rights-filtered corpus](https://github.com/P4suta/aozora-rights-filtered-corpus).
Its automated rights checks cover display in Japan and hosting in the United States and provide no guarantee for other jurisdictions.

The verification and site-generation code, including the vendored `aozora` CSS, uses Apache-2.0 or MIT.
Those licenses do not relicense the texts.
