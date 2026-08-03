# JSONL worker protocol v1

Each distribution adapter is a long-lived worker that reads and writes one JSON object per line.
Standard output is reserved for protocol frames; logs go to standard error.

## Request

```json
{"protocolVersion":1,"requestId":"wasm-0","operation":"render","source":"本文"}
```

## Responses

Success:

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

Failure:

```json
{"protocolVersion":1,"requestId":"wasm-0","ok":false,"error":"message"}
```

All objects reject unknown fields. A span is a half-open byte range:
`{ "start": u64, "end": u64 }`. Adapters may remove only distribution-specific transport framing;
they must preserve HTML and source content, including trailing newlines.

## Artifact input and failure conditions

The artifact manifest pins the worker command, distribution artifact path and SHA-256, adapter
`supportPaths` and SHA-256 values, expected version and schema, and timeout. The host rejects a
missing or mismatched hash, invalid frame, wrong request ID, timeout, abnormal exit, or unexpected
version or schema.
