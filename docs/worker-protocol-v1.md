# JSONL worker protocol v1

各distribution adapterはstdin/stdout上の長寿命JSONL workerです。1行が1frameで、stdoutへlogを出してはいけません。logはstderrへ出します。

Request:

```json
{"protocolVersion":1,"requestId":"wasm-0","operation":"render","source":"本文"}
```

Success response:

```json
{
  "protocolVersion": 1,
  "requestId": "wasm-0",
  "ok": true,
  "result": {
    "version": "0.5.0",
    "schemaVersion": 3,
    "html": "<p>本文</p>",
    "diagnostics": [],
    "gaiji": [],
    "nodes": [],
    "pairs": [],
    "containerPairs": [],
    "source": "本文"
  }
}
```

Failure response:

```json
{"protocolVersion":1,"requestId":"wasm-0","ok":false,"error":"message"}
```

全objectはunknown fieldを許しません。spanは半開byte offset `{ "start": u64, "end": u64 }` です。workerはdistribution固有のtransport framingだけを除去し、HTMLやsourceの末尾改行を一般的な「正規化」として削除してはいけません。

artifact manifestはworker command、実distribution artifactのpath/SHA-256、adapter実行物を含む`supportPaths`のpath/SHA-256、expected version/schema、timeoutを固定します。Rust hostは起動前に全hashを検証し、全作品へ同じprocessを使います。timeout、不正frame、request ID不一致、異常終了、version/schema不一致はその時点で全体を失敗させます。
