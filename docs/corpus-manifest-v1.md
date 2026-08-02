# Rights corpus manifest v1

正式公開で受け付けるmanifestはUTF-8 JSONで、unknown fieldを拒否します。`corpus.commit`、各entryの `upstreamCommit`、実験場のcommitは40桁SHAで固定し、moving branchを参照しません。

```json
{
  "schemaVersion": 1,
  "corpus": {
    "repository": "https://github.com/P4suta/aozora-rights-filtered-corpus",
    "commit": "<40 lowercase hex>",
    "referenceDate": "2026-08-02",
    "cutoffYear": 1930,
    "metadataUrl": "https://www.aozora.gr.jp/...extended_utf8.zip",
    "metadataSha256": "<64 lowercase hex>"
  },
  "entries": [
    {
      "editionId": "000001-<16 lowercase hex>",
      "workId": "000001",
      "title": "作品名",
      "reading": "さくひんめい",
      "copyright": "なし",
      "contributors": [{ "role": "著者", "name": "氏名", "copyright": "なし" }],
      "firstPublication": { "raw": "初出原文", "years": [1929, 1930] },
      "cardUrl": "https://www.aozora.gr.jp/cards/.../card1.html",
      "archiveUrl": "https://www.aozora.gr.jp/cards/.../files/1.zip",
      "archiveFilename": "source.txt",
      "utf8Filename": "edition.txt",
      "upstreamCommit": "<40 lowercase hex>",
      "csvSha256": "<64 lowercase hex>",
      "archiveSha256": "<64 lowercase hex>",
      "utf8Sha256": "<64 lowercase hex>"
    }
  ]
}
```

`editionId` のsuffixは `SHA-256(workId + "\n" + archiveUrl)` の先頭16桁です。これにより作品IDとテキストZIP URLの組を版identityにします。

consumer validatorは次をfail-closedで検査します。

- 作品と全関係者のflagが `なし`
- 初出原文と解析年が空でない
- 全解析年が1000年以上かつcutoff以下
- `cutoffYear = referenceDateの年 - 96`
- 公式HTTPS card/ZIP URLと安全な `.txt` filename
- corpus/entry commit一致、重複edition/identity、全SHA-256
- source読込時のUTF-8、U+FFFD、UTF-8本文hash

初出文字列の和暦・連載年・曖昧表記の解析とCSV/ZIP変換はcorpus generator側の責務です。解析不能な版はentryへ入れず、理由付き隔離manifestへ出します。作品別allowlistは使用しません。
