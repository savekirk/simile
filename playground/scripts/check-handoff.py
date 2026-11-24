"""Verify the actual workspace and bundle downloads from the Playwright suite."""

import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request
import zipfile


ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/debug/simile"
DOWNLOADS = next((ROOT / "playground/test-results").glob("playground-exports*"))


def extract(archive, destination):
    with zipfile.ZipFile(archive) as source:
        for entry in source.namelist():
            if not (destination / entry).resolve().is_relative_to(destination.resolve()):
                raise ValueError("Archive entry escapes the handoff workspace")
        source.extractall(destination)


with tempfile.TemporaryDirectory(prefix="simile-handoff-") as temporary:
    workspace = Path(temporary)
    extract(DOWNLOADS / "workspace.zip", workspace)
    environment = {**os.environ, "SIMILE_HOME": str(workspace / "home")}

    def cli(*arguments):
        result = subprocess.run(
            [str(BINARY), "--output", "json", *arguments],
            cwd=workspace,
            env=environment,
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode:
            raise RuntimeError(f"Native {arguments[0]} failed: {result.stderr}")
        return json.loads(result.stdout)

    cli("inspect", "service.similec")
    cli("describe", "service", "--config", "Simfile")
    cli("register", "--config", "Simfile")
    setup = cli(
        "setup", "orders", "--profile", "browser-handoff", "--set", "name=integration", "--use"
    )
    # Keep issued values in memory and the temporary native home, never in output.
    credentials = {
        key: json.loads(value)
        for key, value in (
            line.split("=", 1)
            for line in Path(setup["credentialsFile"]).read_text().splitlines()
        )
    }
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 4242))
    with (workspace / "server.log").open("w") as log:
        server = subprocess.Popen(
            [str(BINARY), "start", "orders"],
            cwd=workspace,
            env=environment,
            stdout=log,
            stderr=log,
        )
        try:
            for attempt in range(100):
                if server.poll() is not None:
                    raise RuntimeError("Native listener exited before readiness")
                try:
                    with socket.create_connection(("127.0.0.1", 4242), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.1)
            else:
                raise TimeoutError("Native listener was not ready in 10 seconds")
            request = urllib.request.Request(
                "http://127.0.0.1:4242/orders",
                data=b'{"product":"licensed_beat","quantity":2}',
                headers={"Content-Type": "application/json", "X-API-Key": credentials["API_KEY"]},
            )
            with urllib.request.urlopen(request, timeout=5) as response:
                assert response.status == 201
                order = json.load(response)
            request = urllib.request.Request(
                "http://127.0.0.1:4242/orders/" + order["id"],
                headers={"X-API-Key": credentials["API_KEY"]},
            )
            with urllib.request.urlopen(request, timeout=5) as response:
                assert response.status == 200
                assert json.load(response) == order
        finally:
            server.terminate()
            server.wait(timeout=10)
    print("Native registration, setup, and authenticated create/read passed.")
    bundle_root = workspace / "reproduction"
    extract(DOWNLOADS / "bundle.zip", bundle_root)
    bundle = next(bundle_root.rglob("bundle.json")).parent
    replay = cli("replay", str(bundle))
    assert replay["matched"] is True
    print("Native replay of the browser-exported reproduction bundle passed.")
