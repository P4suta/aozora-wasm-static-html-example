# aozora-wasm static HTML example

[`aozora-wasm`](https://www.npmjs.com/package/aozora-wasm)をビルド時に使い、
青空文庫のUTF-8テキストから静的な読書ページを生成する最小のconsumer例です。

- [公開デモ](https://p4suta.github.io/aozora-wasm-static-html-example/)
- [ビルド結果と入力ハッシュ](https://p4suta.github.io/aozora-wasm-static-html-example/build-report.json)

## 境界

このRepoは青空文庫の検索・メタデータ更新・配信サービスを再実装しません。
作品ファイルを「先頭書誌・記号凡例・本文・底本情報」に分け、本文だけを
`aozora-wasm`へ渡し、返されたsemantic HTML断片を完全なHTML5ページへ
組み立てます。

青空文庫記法のルビ、外字、注記は事前のテキスト置換を行いません。
ファイル外枠の分離とページ構築はconsumer、本文記法の解釈はparserの責務です。

## 再現

Node.js 24以降で実行します。

```sh
npm ci
npm test
npm run build
```

`dist/`に索引、10作品の`{作品ID}.utf8.html`、UTF-8原文、
`build-report.json`が生成されます。通常ビルドはネットワークへ接続しません。

入力は青空文庫公式のShift_JIS ZIPをUTF-8へ復号しただけのスナップショットです。
`works.json`に公式作品カード、ZIP URL、権利表示、取得日、ZIPとUTF-8本文の
SHA-256を記録しています。Unicode正規化、外字置換、本文修正は行っていません。

## CSS

`vendor/aozora-notation.css`は`aozora`の任意標準スタイルシートの固定コピーです。
出典コミットとSHA-256は`vendor/aozora-notation.json`に記録しています。
この資産がnpm版へ同梱された後は、npmパッケージからコピーする構成へ移行します。

## 出典とライセンス

作品本文と書誌情報は[青空文庫](https://www.aozora.gr.jp/)に由来します。
収録対象は公式CSVで著作権表示が「なし」の作品だけです。各生成ページから
公式作品カード、配布ZIP、使用したUTF-8原文へ移動できます。

このRepoは青空文庫および各関係者による公式サービスではありません。
変換・サイト生成コードとvendorしたaozora CSSは
Apache License 2.0またはMIT Licenseのデュアルライセンスです。
作品本文をこれらのライセンスで再ライセンスするものではありません。
