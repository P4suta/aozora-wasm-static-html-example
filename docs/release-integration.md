# Release integration

`aozora` calls `.github/workflows/release-gate.yml` at a 40-character commit SHA. All candidate
artifacts and manifests must come from the same `aozora` commit and use the same expected version
and schema.

## Inputs

- `{candidate_artifact_prefix}-linux|macos|windows`: one `bundle.tar`; its root contains
  `artifacts.json` and the pinned distribution artifacts and adapters.
- `{corpus_artifact}`: one `corpus.tar`; its root contains `manifest.json` and UTF-8 texts.
- `{diagnostics_artifact}`: the approved `diagnostics-baseline.json` for the same corpus manifest.
- `{baseline_artifact}`: one `site.tar` containing the approved Pages output.
- `{approval_artifact}`: optional `approval.json` naming every accepted visual difference.

Example:

```yaml
jobs:
  real-work:
    needs: [release-artifacts, rights-corpus, previous-pages]
    uses: P4suta/aozora-wasm-static-html-example/.github/workflows/release-gate.yml@0123456789abcdef0123456789abcdef01234567
    with:
      lab_commit: 0123456789abcdef0123456789abcdef01234567
      aozora_commit: fedcba9876543210fedcba9876543210fedcba98
      candidate_artifact_prefix: aozora-candidate
      corpus_artifact: aozora-rights-corpus
      diagnostics_artifact: aozora-diagnostics-stable
      baseline_artifact: aozora-pages-stable
```

The preparation and verification interface is:

```sh
cargo run --locked --bin xtask -- release prepare \
  --lab-commit 0123456789abcdef0123456789abcdef01234567 \
  --corpus-archive _inputs/corpus/corpus.tar \
  --candidate-archive _inputs/candidate/bundle.tar
cargo run --locked --bin xtask -- release verify --platform linux --shard 0/4
```

## Outputs and failure conditions

Verification produces one report per operating-system shard. After every shard passes, the Linux
candidate is rebuilt without sharding, compared in Chromium, Firefox, and WebKit, and emitted as the
`pages_artifact` workflow output.

The gate fails on an unpinned or mismatched commit, unsafe archive, missing input, artifact or corpus
hash mismatch, worker difference, diagnostics regression, unapproved visual difference, or site
larger than 900 MiB. A bootstrap diagnostics file is review output only and never replaces the
approved baseline automatically.
