# aozora-wasm static HTML example

[`aozora-wasm`](https://www.npmjs.com/package/aozora-wasm)をビルド時に使い、
青空文庫のUTF-8テキストから静的な読書ページを生成するTypeScript製のconsumer例です。
ブラウザへJavaScriptを配信せず、生成済みHTMLとCSSだけをGitHub Pagesで公開します。

- [公開デモ](https://p4suta.github.io/aozora-wasm-static-html-example/)
- [ビルド結果と入力ハッシュ](https://p4suta.github.io/aozora-wasm-static-html-example/build-report.json)

## 境界

このRepoは青空文庫の検索・メタデータ更新・配信サービスを再実装しません。
作品ファイルを「先頭書誌・記号凡例・本文・底本情報」に分け、本文だけを
`aozora-wasm`へ渡し、返されたsemantic HTML断片を完全なHTML5ページへ
組み立てます。

青空文庫記法のルビ、外字、注記は事前のテキスト置換を行いません。
ファイル外枠の分離とページ構築はconsumer、本文記法の解釈はparserの責務です。

## 開発

[Bun 1.3.14](https://bun.sh/)をパッケージ管理、TypeScript実行、テスト、
カバレッジに使用します。依存は完全版番号と`bun.lock`で固定し、新規公開から
3日未満のパッケージをインストール対象から除外しています。

```sh
bun install --frozen-lockfile
bun run ci
```

主な個別コマンドは次のとおりです。

| コマンド | 内容 |
| --- | --- |
| `bun run build` | `dist/`を一時ディレクトリで生成して原子的に置換 |
| `bun test` | manifest、外枠分離、HTML、決定性の単体・統合テスト |
| `bun run typecheck` | TypeScriptのstrict検査 |
| `bun run lint` | Biomeによる静的解析 |
| `bun run dead-code` | Knipによる未使用コード・依存検査 |
| `bun run e2e` | Chromium、Playwright、axeによる表示・アクセシビリティ検査 |

`dist/`には索引、10作品の`{作品ID}.utf8.html`、UTF-8原文、
`build-report.json`が生成されます。同じ入力から二度生成した全ファイルのSHA-256が
一致することをテストしています。通常ビルドはネットワークへ接続しません。

入力manifestとCSS provenanceはZodのstrict schemaで検証します。UTF-8は不正byteを
置換せず拒否し、作品ID、URL、権利表示、重複、入力hash、vendor CSSのhashをビルド前に
検査します。生成先は成功時だけ入れ替えるため、失敗したビルドが既存の`dist/`を
半端な状態にしません。

入力は青空文庫公式のShift_JIS ZIPをUTF-8へ復号しただけのスナップショットです。
`works.json`に公式作品カード、ZIP URL、権利表示、取得日、ZIPとUTF-8本文の
SHA-256を記録しています。Unicode正規化、外字置換、本文修正は行っていません。

## CSS

`vendor/aozora-notation.css`は`aozora`の任意標準スタイルシートの固定コピーです。
出典コミットとSHA-256は`vendor/aozora-notation.json`に記録しています。
この資産がnpm版へ同梱された後は、npmパッケージからコピーする構成へ移行します。

解決できた外字は通常のUnicode文字として本文と同じ色・背景で表示します。原文を
失わず表示する未解決外字だけを識別可能な装飾にするため、このサイト固有のCSSを
標準スタイルシートの後に適用しています。

## 公開版の既知境界

現在はnpmで公開されている`aozora-wasm@0.5.0`を使用します。この版では「蜘蛛の糸」の
`犍陀多《かんだた》`で、外字から解決した「犍」がruby baseの外側に残ります。
このRepoはconsumer側で記法の意味を補正せず、version別のunit testとbrowser testで
挙動を固定しています。修正は`aozora`本体の
[P4suta/aozora#625](https://github.com/P4suta/aozora/pull/625)に含まれており、
公開版を更新した時点でtestはruby base全体が「犍陀多」であることを要求します。

## 継続的検証

Pull Requestでは型、format、lint、カバレッジ、dead code、決定性、HTML構文、
ブラウザ表示、アクセシビリティ、dependency review、CodeQLを検査します。
DependabotはBun依存とGitHub Actionsを週次更新し、Actionsは完全なcommit SHAで
固定しています。脆弱性は[Security policy](.github/SECURITY.md)から非公開で報告できます。

## 出典とライセンス

作品本文と書誌情報は[青空文庫](https://www.aozora.gr.jp/)に由来します。
収録対象は公式CSVで著作権表示が「なし」の作品だけです。各生成ページから
公式作品カード、配布ZIP、使用したUTF-8原文へ移動できます。

このRepoは青空文庫および各関係者による公式サービスではありません。
変換・サイト生成コードとvendorしたaozora CSSは
Apache License 2.0またはMIT Licenseのデュアルライセンスです。
作品本文をこれらのライセンスで再ライセンスするものではありません。
