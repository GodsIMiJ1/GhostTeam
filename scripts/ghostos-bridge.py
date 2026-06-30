#!/usr/bin/env python3
"""Small GhostOS /infer bridge that forwards requests to the real runtime."""

from __future__ import annotations

import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


UPSTREAM_URL = os.environ.get("GHOSTOS_UPSTREAM_URL", "http://ghostos-real:8501/infer")
BRIDGE_PORT = int(os.environ.get("GHOSTOS_BRIDGE_PORT", "9000"))


class BridgeHandler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:
        if self.path != "/infer":
            self.send_json(404, {"error": "not found"})
            return

        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)

        upstream_request = Request(
            UPSTREAM_URL,
            data=body,
            method="POST",
            headers={
                "Content-Type": self.headers.get("Content-Type", "application/json"),
            },
        )

        try:
            with urlopen(upstream_request, timeout=120) as upstream_response:
                payload = upstream_response.read()
                content_type = upstream_response.headers.get(
                    "Content-Type", "application/json"
                )
                self.send_response(upstream_response.status)
                self.send_header("Content-Type", content_type)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
        except HTTPError as error:
            self.send_json(
                error.code,
                {"error": f"upstream returned HTTP {error.code}", "details": error.read().decode("utf-8", "ignore")},
            )
        except URLError as error:
            self.send_json(502, {"error": "failed to reach upstream GhostOS runtime", "details": str(error)})

    def do_GET(self) -> None:
        if self.path == "/health":
            self.send_json(200, {"ok": True})
            return
        self.send_json(404, {"error": "not found"})

    def log_message(self, format: str, *args) -> None:  # noqa: A003
        return

    def send_json(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def main() -> None:
    server = HTTPServer(("0.0.0.0", BRIDGE_PORT), BridgeHandler)
    server.serve_forever()


if __name__ == "__main__":
    main()
