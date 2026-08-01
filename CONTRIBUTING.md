# Contributing

Requirements are Bun 1.3.14 and a Chromium-compatible Playwright environment.

```sh
bun install --frozen-lockfile
bun run ci
```

Do not edit `dist/` by hand. Change the generator or an input, run `bun run build`, and commit the
resulting deterministic output. If an input snapshot changes, record its official source and
SHA-256 in `works.json`.

Keep parser behavior in `P4suta/aozora`. This repository owns only the Aozora file envelope,
static-page composition, and the consumer presentation layer.
