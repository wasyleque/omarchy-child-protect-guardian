#!/usr/bin/env python3
"""guardian-ai sidecar (BETA): runs the local ONNX models and answers guardiand over a Unix socket.

One JSON object per line in, one JSON line out. Ops:
  {"op":"ping"}
  {"op":"rate_package","age":9,"source":"pacman|flatpak|aur|snap|nix","name":"tuxmath"}
      -> {"ok":true,"light":"green|yellow|red|unknown","min_age_hint":7}
  {"op":"check_page","age":9,"url":"..","title":"..","text":".."}      -> {"ok":true,"block":bool,"risk":0.12,"stage":"lite|main"}
  {"op":"grooming","session":"app-or-chat-id","speaker":"A","text":".."} -> {"ok":true,"alert":false} | {"ok":true,"alert":true,"score":0.99}

Safety/privacy: no network (the systemd unit has PrivateNetwork=yes), nothing is logged except op names
and verdicts, text is never written anywhere, chat windows live only in RAM. A package that cannot be
described (not found in the local package database) gets light "unknown" - the daemon then asks the parent
instead of guessing."""
import asyncio, gzip, glob, json, os, re, subprocess, sys, time
import xml.etree.ElementTree as ET

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ocpg_ai import OcpgAI  # noqa: E402

MODELS = os.environ.get("GUARDIAN_AI_MODELS", "/usr/share/guardian-ai/models")
SOCKET = os.environ.get("GUARDIAN_AI_SOCKET", "/run/guardian-ai/ai.sock")
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9@._+-]{0,99}$")   # no leading '-', no shell metacharacters
MAX_LINE = 64 * 1024

ai = OcpgAI(MODELS)
lock = asyncio.Lock()                      # one inference at a time: 1 CPU thread, low priority
monitors = {}                              # chat session -> GroomingMonitor (RAM only)
desc_cache = {}


def _run(cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=10, check=False).stdout
    except Exception:
        return ""


def _pacman_desc(name):
    for cmd in (["pacman", "-Si", name], ["pacman", "-Qi", name]):
        out = _run(cmd)
        m = re.search(r"^Description\s*:\s*(.+)$", out, re.M)
        if m:
            return m.group(1).strip(), []
    return None


def _flatpak_desc(app_id):
    """Stream the local Flathub appstream (no network) until the component is found."""
    for path in glob.glob("/var/lib/flatpak/appstream/*/*/active/appstream.xml.gz") + \
                glob.glob(os.path.expanduser("~/.local/share/flatpak/appstream/*/*/active/appstream.xml.gz")):
        try:
            with gzip.open(path, "rb") as f:
                for _, el in ET.iterparse(f):
                    if el.tag != "component":
                        continue
                    cid = (el.findtext("id") or "").removesuffix(".desktop")
                    if cid == app_id:
                        name = el.findtext("name") or app_id
                        summ = el.findtext("summary") or ""
                        cats = [c.text for c in el.findall("categories/category") if c.text]
                        desc = " ".join("".join(p.itertext()) for p in el.findall("description/p"))[:1200]
                        return f"{name} | {summ} | {desc}", cats
                    el.clear()
        except Exception:
            continue
    return None


def describe(source, name):
    key = (source, name)
    if key not in desc_cache:
        if not NAME_RE.match(name):
            desc_cache[key] = None
        elif source in ("pacman", "aur"):
            desc_cache[key] = _pacman_desc(name)
        elif source == "flatpak":
            desc_cache[key] = _flatpak_desc(name)
        else:
            desc_cache[key] = None
    return desc_cache[key]


def op_rate_package(req):
    age, source, name = int(req["age"]), str(req.get("source", "")), str(req["name"])
    d = describe(source, name)
    if d is None:
        return {"ok": True, "light": "unknown", "min_age_hint": 0}
    text, cats = d
    r = ai.rate_package(age, name, summary=text, categories=cats)
    return {"ok": True, "light": r["light"], "min_age_hint": r["min_age_hint"]}


def op_check_page(req):
    r = ai.check_page(int(req["age"]), req.get("url", ""), req.get("title", ""), req.get("text", ""))
    return {"ok": True, **r}


def op_grooming(req):
    sid = str(req.get("session", "default"))[:80]
    if sid not in monitors:
        if len(monitors) >= 50:
            monitors.pop(next(iter(monitors)))
        monitors[sid] = ai.grooming_monitor()
    res = monitors[sid].push(str(req.get("speaker", "A"))[:20], str(req.get("text", "")))
    return {"ok": True, "alert": False} if res is None else {"ok": True, **res}


OPS = {"ping": lambda r: {"ok": True}, "rate_package": op_rate_package, "check_page": op_check_page, "grooming": op_grooming}


async def handle(reader, writer):
    try:
        line = await asyncio.wait_for(reader.readline(), 5)
        if not line or len(line) > MAX_LINE:
            raise ValueError("bad request")
        req = json.loads(line)
        op = OPS.get(req.get("op"))
        if op is None:
            raise ValueError("unknown op")
        async with lock:
            reply = await asyncio.get_running_loop().run_in_executor(None, op, req)
        print(f"op={req['op']} " + " ".join(f"{k}={v}" for k, v in reply.items() if k in ("light", "min_age_hint", "block", "alert")), flush=True)
    except Exception as e:                 # never echo input back
        print(f"error: {type(e).__name__}", flush=True)
        reply = {"ok": False}
    writer.write((json.dumps(reply) + "\n").encode())
    await writer.drain()
    writer.close()


async def janitor():
    while True:
        await asyncio.sleep(60)
        ai.unload_idle(300)                # free RAM when idle


async def main():
    try:
        os.unlink(SOCKET)
    except FileNotFoundError:
        pass
    os.makedirs(os.path.dirname(SOCKET), exist_ok=True)
    server = await asyncio.start_unix_server(handle, SOCKET)
    os.chmod(SOCKET, 0o600)                # the daemon runs as root and can connect; the child cannot
    asyncio.create_task(janitor())
    print(f"guardian-ai listening on {SOCKET}, models {MODELS}", flush=True)
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    asyncio.run(main())
