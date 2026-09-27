#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "aiocoap>=0.4.7",
# ]
# ///
"""Get or set makit64 panel brightness via CoAP.

Usage:
  uv run tools/set_brightness.py              # GET current (0–255)
  uv run tools/set_brightness.py 128          # PUT level
  uv run tools/set_brightness.py -H 192.168.0.42 64
"""

from __future__ import annotations

import argparse
import asyncio
import sys

try:
    from aiocoap import Context, Message
    from aiocoap.numbers.codes import GET, PUT
except ImportError as e:
    sys.stderr.write(
        f"Missing dependency: {e}\n"
        "Run with: uv run tools/set_brightness.py …\n"
    )
    sys.exit(1)

DEFAULT_HOST = "makit.local"


async def brightness(host: str, port: int, level: int | None) -> None:
    context = await Context.create_client_context()
    uri = f"coap://{host}:{port}/brightness"
    if level is None:
        request = Message(code=GET, uri=uri)
    else:
        request = Message(code=PUT, uri=uri, payload=str(level).encode())
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
    parser.add_argument(
        "level",
        nargs="?",
        type=int,
        help="Brightness 0–255 (omit to GET current)",
    )
    args = parser.parse_args()
    if args.level is not None and not 0 <= args.level <= 255:
        raise SystemExit("level must be 0–255")
    asyncio.run(brightness(args.host, args.port, args.level))


if __name__ == "__main__":
    main()
