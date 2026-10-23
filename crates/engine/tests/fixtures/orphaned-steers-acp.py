#!/usr/bin/env python3
"""File-gated ACP peer for offline/EOF/crash recovery through the real adapter."""
import json
import os
from pathlib import Path
import select
import sys
import time

buffer = b""
root = None


def emit(value):
    print(json.dumps({"jsonrpc": "2.0", **value}), flush=True)


def reply(request, result):
    emit({"id": request["id"], "result": result})


def read(timeout=None):
    global buffer
    deadline = None if timeout is None else time.monotonic() + timeout
    while b"\n" not in buffer:
        left = None if deadline is None else max(0, deadline - time.monotonic())
        if not select.select([sys.stdin], [], [], left)[0]:
            return None
        data = os.read(sys.stdin.fileno(), 65536)
        if not data:
            sys.exit(0)
        buffer += data
    line, buffer = buffer.split(b"\n", 1)
    return json.loads(line)


def log(event):
    with (root / "acp-events.jsonl").open("a") as file:
        file.write(json.dumps(event) + "\n")


def text(value):
    emit({"method": "session/update", "params": {
        "sessionId": "fixture-session",
        "update": {"sessionUpdate": "agent_message_chunk", "content": {
            "type": "text", "text": value,
        }},
    }})


def wait_for_gate(name, prompt):
    deadline = time.monotonic() + 20
    while not (root / name).exists():
        if time.monotonic() > deadline:
            raise RuntimeError("test did not release the ACP gate")
        incoming = read(0.01)
        if incoming is None:
            continue
        if incoming.get("method") == "session/cancel":
            reply(prompt, {"stopReason": "cancelled"})
            return False
        if "id" in incoming:
            reply(incoming, {})
    return True


while True:
    request = read()
    method = request.get("method")
    if method == "initialize":
        reply(request, {"protocolVersion": 1, "agentCapabilities": {
            "loadSession": True, "promptCapabilities": {"image": True},
        }})
    elif method in ("session/new", "session/load"):
        root = Path(request["params"]["cwd"])
        log({"event": "session", "method": method})
        reply(request, {"sessionId": "fixture-session"})
    elif method == "session/prompt":
        prompt = "".join(part.get("text", "") for part in request["params"]["prompt"])
        log({"event": "prompt", "text": prompt, "content": request["params"]["prompt"]})
        mode = (root / "failure-mode").read_text().strip()
        fails = prompt == "opening" and mode != "next_error"
        if prompt == "opening" or (prompt == "first" and mode == "next_error"):
            text("working: " + prompt)
            gate = "release-second" if prompt == "first" else "release-first"
            if not wait_for_gate(gate, request):
                continue
            fails = fails or prompt == "first"
        if fails:
            if mode == "eof":
                sys.exit(0)
            if mode == "crash":
                print("simulated offline transport failure", file=sys.stderr, flush=True)
                sys.exit(17)
            emit({"id": request["id"], "error": {
                "code": -32603,
                "message": "network error: UnknownHostName (gave up after 6 attempts)",
            }})
        else:
            text("ack: " + prompt)
            reply(request, {"stopReason": "end_turn"})
    elif "id" in request:
        reply(request, {})
