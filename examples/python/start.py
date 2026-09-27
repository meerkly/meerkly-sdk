#!/usr/bin/env python3
"""Minimal: start sharing bandwidth in the background (Python).

    PYTHONPATH=sdk/python python3 examples/python/start.py
"""
import asyncio
import os

from meerkly import ProxyClient, ProxyConfig


async def main():
    client = ProxyClient(ProxyConfig(
        publisher_id=os.environ.get("MEERKLY_PUBLISHER_ID", "your-publisher-id"),
        gateway_addresses=["127.0.0.1:4443"],  # dev override (prod bakes this in)
        ca_cert_path="certs/dev/ca.crt",        # dev override
    ))

    await client.start()  # connects + registers; the exit node now runs in the background
    print("sharing bandwidth as", client.client_key())

    # Keep the program alive; the proxy works in the background.
    try:
        await asyncio.Event().wait()
    except (KeyboardInterrupt, asyncio.CancelledError):
        await client.stop()


if __name__ == "__main__":
    asyncio.run(main())
