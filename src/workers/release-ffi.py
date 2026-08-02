from __future__ import annotations

import ctypes
import json
import sys
from pathlib import Path
from typing import Any


class AozoraBytes(ctypes.Structure):
    _fields_ = [
        ("ptr", ctypes.c_void_p),
        ("len", ctypes.c_size_t),
        ("cap", ctypes.c_size_t),
    ]


def request(value: object) -> dict[str, Any]:
    if not isinstance(value, dict) or sorted(value) != [
        "operation",
        "protocolVersion",
        "requestId",
        "source",
    ]:
        raise ValueError("invalid request envelope")
    if (
        value["protocolVersion"] != 1
        or value["operation"] != "render"
        or not isinstance(value["requestId"], str)
        or not value["requestId"]
        or not isinstance(value["source"], str)
    ):
        raise ValueError("invalid request fields")
    return value


if len(sys.argv) != 4:
    raise SystemExit("usage: release-ffi.py <library> <version> <schema-version>")

library = ctypes.CDLL(str(Path(sys.argv[1]).resolve()))
version = sys.argv[2]
schema_version = int(sys.argv[3])
library.aozora_document_new.argtypes = [
    ctypes.c_void_p,
    ctypes.c_size_t,
    ctypes.POINTER(ctypes.c_void_p),
]
library.aozora_document_new.restype = ctypes.c_int
library.aozora_document_free.argtypes = [ctypes.c_void_p]
library.aozora_bytes_free.argtypes = [AozoraBytes]

projections = {
    "html": "aozora_document_to_html",
    "diagnostics": "aozora_document_diagnostics_json",
    "gaiji": "aozora_document_gaiji_json",
    "nodes": "aozora_document_nodes_json",
    "pairs": "aozora_document_pairs_json",
    "containerPairs": "aozora_document_container_pairs_json",
    "source": "aozora_document_to_source",
}
for symbol in projections.values():
    function = getattr(library, symbol)
    function.argtypes = [ctypes.c_void_p, ctypes.POINTER(AozoraBytes)]
    function.restype = ctypes.c_int


def output(document: ctypes.c_void_p, symbol: str) -> str:
    value = AozoraBytes()
    status = getattr(library, symbol)(document, ctypes.byref(value))
    if status != 0:
        raise RuntimeError(f"{symbol} failed with status {status}")
    try:
        return ctypes.string_at(value.ptr, value.len).decode("utf-8")
    finally:
        library.aozora_bytes_free(value)


def render(source: str) -> dict[str, Any]:
    encoded = source.encode("utf-8")
    buffer = ctypes.create_string_buffer(encoded)
    document = ctypes.c_void_p()
    status = library.aozora_document_new(buffer, len(encoded), ctypes.byref(document))
    if status != 0:
        raise RuntimeError(f"aozora_document_new failed with status {status}")
    try:
        result: dict[str, Any] = {"version": version, "schemaVersion": schema_version}
        for name, symbol in projections.items():
            value = output(document, symbol)
            if name not in {"html", "source"}:
                envelope = json.loads(value)
                if envelope["schemaVersion"] != schema_version:
                    raise RuntimeError(f"{symbol} schema mismatch")
                value = envelope["data"]
            result[name] = value
        return result
    finally:
        library.aozora_document_free(document)


for line in sys.stdin:
    request_id = "invalid"
    try:
        item = request(json.loads(line))
        request_id = item["requestId"]
        response = {
            "protocolVersion": 1,
            "requestId": request_id,
            "ok": True,
            "result": render(item["source"]),
        }
    except Exception as error:  # noqa: BLE001
        response = {
            "protocolVersion": 1,
            "requestId": request_id,
            "ok": False,
            "error": str(error),
        }
    print(json.dumps(response, ensure_ascii=False, separators=(",", ":")), flush=True)
