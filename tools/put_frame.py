#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "aiocoap>=0.4.7",
#     "pillow>=10",
# ]
# ///
"""Resize an image to 64×64 RGB888 and CoAP PUT it to makit64.

Usage:
  uv run tools/put_frame.py tools/tiger.png
  uv run tools/put_frame.py -H 192.168.0.42 tools/tiger.png
"""

from __future__ import annotations

import argparse
import asyncio
import sys

try:
    from aiocoap import Context, Message
    from aiocoap.numbers.codes import PUT
    from PIL import Image
except ImportError as e:
    sys.stderr.write(
        f"Missing dependency: {e}\n"
        "Run with: uv run tools/put_frame.py …\n"
    )
    sys.exit(1)

PANEL = 64
FRAME_BYTES = PANEL * PANEL * 3
DEFAULT_HOST = "makit.local"


def to_rgb888(path: str) -> bytes:
    img = Image.open(path).convert("RGB")
    img = img.resize((PANEL, PANEL), Image.Resampling.LANCZOS)
    data = img.tobytes()
    if len(data) != FRAME_BYTES:
        raise SystemExit(f"unexpected RGB size {len(data)} (want {FRAME_BYTES})")
    return data


async def put_frame(host: str, port: int, rgb: bytes) -> None:
    context = await Context.create_client_context()
    uri = f"coap://{host}:{port}/frame"
    # aiocoap performs Block1 automatically for large payloads.
    request = Message(code=PUT, uri=uri, payload=rgb)
    response = await context.request(request).response
    print(f"{uri} → {response.code} {response.payload!r}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "-H",
        "--host",
        default=DEFAULT_HOST,
        help=f"Device hostname or IPv4 (default: {DEFAULT_HOST})",
    )
    parser.add_argument("--port", type=int, default=5683)
    parser.add_argument("image", help="Input image path")
    args = parser.parse_args()

    rgb = to_rgb888(args.image)
    asyncio.run(put_frame(args.host, args.port, rgb))


if __name__ == "__main__":
    main()
