from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any


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


if len(sys.argv) != 2:
    raise SystemExit("usage: release-python.py <extracted-wheel>")
sys.path.insert(0, str(Path(sys.argv[1]).resolve()))
import aozora  # type: ignore[import-not-found]  # noqa: E402

for line in sys.stdin:
    request_id = "invalid"
    try:
        item = request(json.loads(line))
        request_id = item["requestId"]
        document = aozora.Document(item["source"])
        result = {
            "version": aozora.version(),
            "schemaVersion": aozora.schema_version(),
            "html": document.to_html(),
            "diagnostics": json.loads(document.diagnostics_json())["data"],
            "gaiji": json.loads(document.gaiji_json())["data"],
            "nodes": json.loads(document.nodes_json())["data"],
            "pairs": json.loads(document.pairs_json())["data"],
            "containerPairs": json.loads(document.container_pairs_json())["data"],
            "source": document.to_source(),
        }
        response = {
            "protocolVersion": 1,
            "requestId": request_id,
            "ok": True,
            "result": result,
        }
    except Exception as error:  # noqa: BLE001
        response = {
            "protocolVersion": 1,
            "requestId": request_id,
            "ok": False,
            "error": str(error),
        }
    print(json.dumps(response, ensure_ascii=False, separators=(",", ":")), flush=True)
