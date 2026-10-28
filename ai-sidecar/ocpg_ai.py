"""OCPG local AI models - light runtime (onnxruntime + tokenizers + numpy only; no torch, no GPU).

  from ocpg_ai import OcpgAI
  ai = OcpgAI("release")                       # nothing is loaded yet
  ai.check_page(age=9, url=u, title=t, text=body)   -> {"block": bool, "risk": 0..1, "stage": "lite"|"main"}
  ai.rate_package(age=9, name=n, summary=s)         -> {"light": "green|yellow|red", "min_age_hint": 12, "probs": [...]}
  mon = ai.grooming_monitor(); mon.push("A", "msg") -> None | {"alert": True, "score": 0.99}

Privacy: nothing is stored or logged here; the grooming monitor keeps only the last 12 messages in RAM.
Resources: one thread per session, sessions loaded lazily and released by unload_idle()."""
import json, os, time
from collections import deque
import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

PROXY = [0, 7, 12, 16, 18]


class _Model:
    def __init__(self, path, max_len):
        self.path, self.max_len, self.sess, self.tok, self.used = path, max_len, None, None, 0.0

    def _load(self):
        so = ort.SessionOptions(); so.intra_op_num_threads = 1; so.inter_op_num_threads = 1
        so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        self.sess = ort.InferenceSession(os.path.join(self.path, "model.int8.onnx"), so, providers=["CPUExecutionProvider"])
        self.tok = Tokenizer.from_file(os.path.join(self.path, "tokenizer.json"))
        self.tok.enable_truncation(self.max_len)

    def probs(self, text):
        if self.sess is None:
            self._load()
        self.used = time.monotonic()
        e = self.tok.encode(text)
        ids = np.array([e.ids], dtype=np.int64)
        lg = self.sess.run(None, {"input_ids": ids, "attention_mask": np.ones_like(ids)})[0][0]
        x = np.exp(lg - lg.max())
        return (x / x.sum()).tolist()

    def unload(self):
        self.sess = self.tok = None


class GroomingMonitor:
    """Sliding window over the last `window` chat messages; alert after `need` consecutive windows above threshold."""
    def __init__(self, model, cfg):
        self.m, self.cfg = model, cfg
        self.buf, self.n, self.hits = deque(maxlen=cfg["window"]), 0, 0

    def push(self, speaker, text):
        self.buf.append(f"{speaker}: {text.replace(chr(10), ' ')[:200]}")
        self.n += 1
        if len(self.buf) < self.cfg["window"] or self.n % self.cfg["stride"]:
            return None
        score = self.m.probs("\n".join(self.buf))[1]
        self.hits = self.hits + 1 if score >= self.cfg["threshold"] else 0
        if self.hits >= self.cfg["need_consecutive"]:
            self.hits = 0
            return {"alert": True, "score": round(score, 3)}   # only the score leaves this module, never the text
        return None

    def reset(self):
        self.buf.clear(); self.n = self.hits = 0


class OcpgAI:
    def __init__(self, root="release"):
        self.root, self.cfg = root, json.load(open(os.path.join(root, "config.json")))
        mk = lambda name, ml: _Model(os.path.join(root, name), ml)
        self.lite = mk("content_lite", 384)
        self.mains = {"xlmr": mk("content_main_xlmr", 384), "fast": mk("content_main_fast", 384)}
        self.main = self.mains[self.cfg["content"].get("main_model", "xlmr")]   # "xlmr" = more accurate (265 MB), "fast" = smaller/faster (130 MB)
        self.pkg, self.groom = mk("pkg", 256), mk("grooming", 384)

    @staticmethod
    def _risk(p):
        return p[2] + 0.5 * p[1]

    def check_page(self, age, url="", title="", text=""):
        s = f"Age: {age}\nURL: {url}\nTitle: {title}\n{text[:1500]}"
        c = self.cfg["content"]
        r = self._risk(self.lite.probs(s))
        if r < c["lite_pass_below"]:
            return {"block": False, "risk": round(r, 3), "stage": "lite"}
        r = self._risk(self.main.probs(s))
        return {"block": r > c["main_block_above"], "risk": round(r, 3), "stage": "main"}

    def rate_package(self, age, name, summary="", description="", categories=(), keywords=()):
        s = f"{name} | {summary} | categories: {', '.join(categories)} | keywords: {', '.join(keywords)} | {description[:1200]}"
        p = self.pkg.probs(s)
        b = max([c for c in range(5) if p[c] >= self.cfg["pkg"]["conservative_p"]], default=p.index(max(p)))
        need = PROXY[b]
        light = "green" if need <= age else ("yellow" if need <= age + 3 else "red")
        return {"light": light, "min_age_hint": need, "probs": [round(x, 3) for x in p]}   # advisory: the parent always decides

    def grooming_monitor(self):
        return GroomingMonitor(self.groom, self.cfg["grooming"])

    def unload_idle(self, seconds=300):
        for m in (self.lite, *self.mains.values(), self.pkg, self.groom):
            if m.sess is not None and time.monotonic() - m.used > seconds:
                m.unload()
