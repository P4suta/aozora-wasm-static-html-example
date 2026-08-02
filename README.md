# aozora real-work distribution verification lab

権利判定済みの青空文庫実作品を使い、`aozora` の7配布面を同じ入力・同じschemaで比較する静的サイト兼リリースゲートです。検証とサイト生成の中核はRust製の `lab` CLI、WASM hostとブラウザ検証はTypeScriptで実装しています。公開ページへクライアントJavaScriptは配信しません。

- [公開サイト](https://p4suta.github.io/aozora-wasm-static-html-example/)
- [公開サイトのビルドレポート](https://p4suta.github.io/aozora-wasm-static-html-example/build-report.json)

## 現在の境界

リポジトリ内の `works.json` と10作品は開発用legacy corpusです。実作品でquick/full、決定性、静的出力を検証できますが、初出年と全関係者の権利根拠を備えた正式なrights-filtered manifestではありません。`lab full` と正式公開ジョブは `--require-rights-filtered` により、このlegacy入力を必ず拒否します。

正式ゲートには、別リポジトリ `P4suta/aozora-rights-filtered-corpus` が生成する固定commitのmanifestとUTF-8本文を `--corpus` で渡します。manifest contractは[corpus-manifest-v1](docs/corpus-manifest-v1.md)にあります。

## なぜRustか

検証中核にはGoではなくRustを採用しました。`aozora` 本体とwire schemaがRustで定義されているため変更追従が容易で、Rust配布面に余分なC ABI/cgo境界を増やさず、Linux・macOS・Windowsへ同一のnative CLIを配布できます。Go SDKは検証対象の1配布面として独立workerに保ちます。

各配布面はversioned JSONL workerとして起動され、1作品ごとに次の `EngineResult` を返します。

- `version` / `schemaVersion`
- semantic HTML
- diagnostics / gaiji / nodes / pairs / container-pairs
- `to_source` round-trip

`all` はWASMを正本とし、全fieldをbyte単位で比較します。workerの欠落、artifact hash不一致、異版混在、不正JSON、余分なfield、停止、timeout、出力差はすべてfail-closedです。CLI固有の末尾改行は各workerがpayloadへ入れる前に処理し、Rust transportはJSONL framing以外を変更しません。protocolの詳細は[worker-protocol-v1](docs/worker-protocol-v1.md)を参照してください。

## 開発

Bun 1.3.14とRust 1.97.1を固定しています。

```sh
bun install --frozen-lockfile
bun run check
```

主なコマンドは次のとおりです。

| コマンド | 内容 |
| --- | --- |
| `bun run lab` | 引数なしの決定的なWASM quick検証 |
| `bun run lab doctor --engine all` | host tool、worker、候補artifactとSHA-256を検査 |
| `bun run lab verify --engine wasm --scope quick` | 特徴集合をgreedy coverする実作品quick検証 |
| `bun run lab verify --engine all --scope full --shard 0/4` | 全配布面・全作品のshard検証 |
| `bun run lab build --engine wasm` | 検証後に静的サイトを原子的に生成 |
| `bun run lab visual --baseline PATH --candidate PATH` | 3 browser × 狭幅/広幅の画像・layout比較 |
| `bun run lab full --baseline PATH --corpus PATH` | rights-filtered corpusに対するhost上の全工程 |
| `bun run xtask release ...` | artifact展開、release shard、Pages候補、fan-inをRustで編成 |

外部workerは `lab/artifacts.json` に固定します。ローカル開発時だけ `AOZORA_LAB_RUST_WORKER='["./worker"]'` のようなJSON command arrayで上書きできます。正式ゲートはrelease-readyが生成した同一commitのartifactとmanifestを使い、overrideを使いません。

現在の固定npm/WASMを使った例:

```sh
bun run lab doctor --engine wasm
bun run lab verify --engine wasm --scope full
bun run lab build --engine wasm
```

## 検証規則

quick corpusは本文中のruby、外字、注記、container、range styleを決定的に抽出し、集合を覆う最小寄りの作品集合をedition IDでtie-breakして選びます。fullはmanifestの全版を処理します。`--shard i/n` は選択後の安定した順序へ適用するため、ローカルとCIが同じ分割規則を共有します。

diagnosticsは `lab/diagnostics-baseline.json` と構造比較します。追加・変形は失敗し、解消だけを改善としてreportへ残します。baselineの自動更新やwildcard承認はありません。

visual検証は同一browser process内でbaseline/candidateを開き、各作品について決定的なcontact sheetと本文全DOMのcomputed style、位置、寸法、改行rect、overflow、ruby配置を比較します。candidateにはaxeも実行します。差分画像は失敗時だけ保存されます。意図した変更は、影響editionの完全な一覧、before/after Merkle root、理由、issueを含む承認manifestと完全一致した場合だけ受理されます。

## 静的出力

`lab build` は検証がすべて通った後だけ一時directoryを置換し、失敗時は既存の `dist/` を保ちます。

- `/works/{editionId}.html`
- `/sources/{editionId}.txt`
- `/reports/{editionId}.json`
- `/diagnostics-baseline.json`（次の正式版が比較する作品別diagnostics）
- `/indexes/authors/index.html`
- `/indexes/gojuon/index.html`
- `/indexes/pages/{page}.html`
- `/build-report.json`

各作品ページはdiagnostics、採用根拠、公式作品カード、公式ZIP、UTF-8原文、比較report、[青空文庫収録ファイルの取り扱い規準](https://www.aozora.gr.jp/guide/kijyunn.html)を表示します。900MiBを超える場合は作品を削らずdeployを停止します。

Pagesを更新できるのは週次・手動の`stable channel` workflowだけです。このworkflowはGitHub Release、npm、crates.io、PyPIで同じstable versionが公開済みであることを解決し、各registryから実際のpackageを再取得して全ゲートを通したサイトだけをdeployします。PRの`site preview`は開発用artifactを保存しますが公開しません。

## 品質ゲート

`bun run check` はBiome、TypeScript、Clippy、Rust/Bunテスト、dead-code、full static buildを実行します。Rust testは権利cutoff、著作権flag、hash、worker fail-closed、projection差、shard、静的サイトの再現性、容量失敗時の原子的保持を検査します。Playwright E2EはChromium・Firefox・WebKitで静的サイトとaccessibilityを検査します。

release-readyとの接続条件とartifact配置は[release-integration](docs/release-integration.md)にあります。
workflow内の条件分岐、tar展開、複数工程の呼び出しは`rust/xtask.rs`へ集約し、YAMLにshell scriptを持たせません。

## 出典とライセンス

作品本文と書誌情報は[青空文庫](https://www.aozora.gr.jp/)に由来します。このサイトは青空文庫および各関係者による公式サービスではなく、自動判定は日本での表示と米国でのhostingを対象にした保守的な運用基準であり、全法域への保証ではありません。

検証・生成コードとvendorしたaozora CSSはApache License 2.0またはMIT Licenseです。作品本文をこれらのライセンスで再ライセンスするものではありません。
