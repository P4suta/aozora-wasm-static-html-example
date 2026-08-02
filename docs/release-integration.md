# Release integration

`aozora` の `release-ready` は `.github/workflows/release-gate.yml` を40桁commit SHAで呼び出します。同一commitから生成した次の候補を、OS別artifact bundleとしてこのlabへ渡します。

- npm tgz / wasm-bindgen worker
- crateからbuildしたRust worker
- native CLI worker
- C archiveとFFI host worker
- Extism wasmとhost worker
- wheel/sdistとPython worker
- Go SDK tarとGo worker

bundle内の `artifacts.json` は各実artifactのSHA-256、adapter実行物の`supportPaths` SHA-256、同じexpected version/schema、worker commandを記録します。workerが別のdistributionを内部で呼ぶ構成は禁止です。

再利用可能workflowへの入力artifactは次の固定形式です。tarで包むことで、GitHub artifact転送でnative executableの実行bitを失いません。

- `{candidate_artifact_prefix}-linux|macos|windows`: `bundle.tar`を1個含み、展開後のrootに`artifacts.json`を持つ。
- `{corpus_artifact}`: `corpus.tar`を1個含み、展開後のrootに`manifest.json`とUTF-8本文を持つ。
- `{diagnostics_artifact}`: 前回承認済み版の`diagnostics-baseline.json`を1個含む。corpus manifest SHAが一致しないbaselineは拒否される。
- `{baseline_artifact}`: 前回承認済みのPages出力を収めた`site.tar`を1個含む。
- `{approval_artifact}`（任意）: 意図したvisual差分だけを列挙する`approval.json`を1個含む。差分がなければ指定しない。

呼び出し例です。tagやbranchではなく、labも40桁SHAで固定します。

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
      # approval_artifact: aozora-visual-approval-123
```

候補jobはLinux・macOS・Windowsで同じxtaskを呼びます。commit照合、安全なtar展開、doctor、全engine比較、report出力はRust側にあり、CI独自の選択・比較ロジックやshell scriptは持ちません。

```sh
cargo run --locked --bin xtask -- release prepare \
  --lab-commit 0123456789abcdef0123456789abcdef01234567 \
  --corpus-archive _inputs/corpus/corpus.tar \
  --candidate-archive _inputs/candidate/bundle.tar
cargo run --locked --bin xtask -- release verify \
  --platform linux \
  --shard 0/4
```

workflowはLinux・macOS・Windowsを4 shardずつ実行し、全OS/shardとvisual jobを`real-work release gate`へfan-inします。`aozora`側の既存`release-ready`はこのjobを必須needsに加えます。このfan-inより前にcrate、npm、PyPI、GitHub Releaseをpublishしてはいけません。

Pages用buildはLinux候補で全engineのunsharded verificationを再実行し、WASMを正本として生成します。900MiB gateを通ったartifactだけをChromium・Firefox・WebKitの狭幅/広幅でvisual baselineと比較します。成功時だけworkflow output `pages_artifact` が指す候補サイトartifactを生成します。候補サイトrootの`diagnostics-baseline.json`は次の正式版入力です。

`.github/workflows/stable.yml`は公開済みGitHub Releaseのtag/commitを正本にし、npm、crates.io、PyPIに同じstable versionが存在することを`xtask release stable-resolve`で確認します。Linux・macOS・Windowsごとに`stable-fetch`がGitHub Release、npm、crates.io、PyPIから実packageを取得し、公開checksumとregistry checksumを検証してbundle化します。同じrelease gateが成功した場合だけ、このworkflowがPagesへdeployします。開発用`site preview`にはPages書き込み権限がありません。

最初のrights-filtered releaseだけは、全作品のdiagnosticsをbootstrap候補として生成し、corpus SHA・全entry・既知diagnosticsをレビューしてから`diagnostics_artifact`へ固定します。空baselineへの自動置換や、失敗jobからのbaseline採用は行いません。

bootstrap候補は固定済みcorpusとcandidate bundleを展開した後、`cargo run --locked --bin xtask -- release bootstrap-diagnostics --engine all`で生成します。このコマンドは全作品・全指定engineのparityを先に証明し、全edition（diagnosticsが空の作品を含む）を列挙したreview用JSONだけを書きます。通常の`release verify`はこのコマンドを呼ばず、承認済みbaselineがなければ失敗します。

このリポジトリの `lab/artifacts.json` は開発用WASM候補だけを固定しています。7面の正式artifact manifestは `aozora` 側release-readyが生成し、このリポジトリへcommitしません。
