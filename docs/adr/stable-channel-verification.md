---
date: 2026-10-21
---

# Stable-channel verification

## Status

Accepted.

## Context

The reusable release gate verifies candidate distributions, corpus provenance, diagnostics, and visual differences for upstream releases.
The weekly stable-channel workflow repeats the full three-platform verification and deployment without a new release request.
Ordinary quality and source-analysis jobs also repeat after a merge, and source analysis repeats weekly without a source change.

## Decision

Run quality checks and source analysis on pull requests and merge groups.
Keep the Pages deployment trigger and reusable release acceptance gate.
Run the complete stable-channel workflow only through an explicit manual request.
Use its installed Rust toolchain for every Cargo command and remove duplicate setup.
Run pinned actionlint in the ordinary quality job to reject invalid workflow expressions.
Keep the reusable gate's public site-artifact output linked to the visual job's actual output name.

## Consequences

A dependency pull request keeps the existing quality and security checks.
Merges avoid duplicate verification, and weekly clocks no longer launch a full release comparison.
Maintainers can still request the stable-channel check before accepting a release.
The change grants no baseline approval or release publication.

Keeping the weekly full matrix would repeat release acceptance without selecting a changed contract.
Removing the reusable release gate would discard required distribution and visual evidence.
Both alternatives are rejected.
