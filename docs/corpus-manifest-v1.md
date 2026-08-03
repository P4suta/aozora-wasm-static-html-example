# Rights corpus manifest v1

Consumers receive a UTF-8 `manifest.json` and the UTF-8 text file named by each entry. Unknown JSON
fields are rejected. Every commit is a lowercase 40-character SHA, not a branch or tag.

## Schema

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

`editionId` ends with the first 16 characters of `SHA-256(workId + "\n" + archiveUrl)`.

## Failure conditions

Consumers reject the corpus when:

- a work or contributor `copyright` value is not `なし`;
- publication text or parsed years are missing, or a year is below 1000 or above `cutoffYear`;
- `cutoffYear` is not the `referenceDate` year minus 96;
- a card URL, ZIP URL, or `.txt` filename is unsafe;
- corpus and entry commits differ, an edition or identity is duplicated, or a hash differs; or
- a text file is invalid UTF-8, contains U+FFFD, or does not match `utf8Sha256`.

The corpus generator parses Japanese era dates and ambiguous publication strings. Unparsable
editions go to a quarantine manifest with a reason; per-work allowlists are not supported.
