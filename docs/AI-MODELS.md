# Local AI models and parent-selectable autonomy (BETA, v0.4.0)

> **Status: beta.** Nobody relies on this yet. It is off by default (`mode = "ask"` = the guardian behaves
> exactly as before). Read "What this means for you" before switching it on.

## Streszczenie (PL)

Opcjonalnie małe, lokalne modele (ONNX, tylko CPU, bez internetu) mogą **odpowiadać na prośby o instalację
zamiast rodzica**, a **wieczorem rodzic zawsze dostaje podsumowanie** z tym, co model zdecydował i co się
działo. Rodzic wybiera tryb: `ask` (domyślnie, wszystko do rodzica), `quiet` (model zatwierdza tylko
„zielone”, resztę pyta rodzica), `auto` (model decyduje; „żółte” i „czerwone” odrzuca). Przy każdym wątpieniu
(model nie działa, nie zna paczki, komunikator/VPN/Tor) decyzja wraca do rodzica. **Alert grooming nigdy nie
czeka do wieczora.** Szczegóły, ryzyka i ograniczenia poniżej (po angielsku, jak reszta dokumentacji).

## What it is

| Part | Where | Job |
|---|---|---|
| `guardian-ai` sidecar | `ai-sidecar/` (Python + onnxruntime) | runs the models, sandboxed: no network, lowest priority, 1 thread, ~750 MB models on disk, ~1 GB RAM when everything is loaded, idle models are unloaded after 5 min |
| `autonomy` module | `daemon/src/autonomy.rs` | turns the model's traffic light + the parent's mode into *decide* or *ask the parent* |
| Evening digest | `guardiand` | sent every day in `quiet`/`auto` at `digest_hour`, even when nothing happened; lists every model decision |
| Models | built separately (distilled from Bielik/Qwen teachers, int8 ONNX) | content text (profanity, insults, sexual, violence, drugs, gambling, self-harm, hate), package age rating, grooming chat windows |

## Modes (`guardian-ctl autonomy ask|quiet|auto`, persisted)

| Mode | green | yellow | red | unknown / sidecar down / always-ask list |
|---|---|---|---|---|
| `ask` (default) | parent | parent | parent | parent |
| `quiet` | **model allows** | parent | parent | parent |
| `auto` | **model allows** | model denies (`yellow = "ask"` to ask instead) | **model denies** | parent |

Rules that hold in every mode:
- A "green" whose age hint is above the child's age is treated as yellow (stricter wins).
- Names in `always_ask` (messengers, Tor/VPN, hacking tools by default) always go to the parent.
- A package the sidecar cannot describe from the local package database is `unknown` → the parent decides.
- Every model decision is written to the hash-chained audit log as `auto_decided` (package, colour, age hint, mode; no other data).
- **Undo a wrong denial:** `guardian-ctl autonomy ask-about <name>` – the child asks again and the parent gets the normal card.
- **Grooming alerts are never delayed, batched or muted** (see "Not wired yet" for the current state).

## The evening digest

Sent as a signed alert (same channel as the existing digest) every day at `[autonomy].digest_hour` (default 19:00) when the mode is
`quiet` or `auto`. It contains the factual day summary (installs, blocked attempts, concerning lookups, apps used)
plus *"AI model decisions: approved … denied … sent to you N of M requests"*. If there is nothing to report it
still says so ("No install requests today"), so silence never means a broken system. If the LLM narrative
([ai] Ollama) is unavailable, the plain facts are sent instead. If delivery fails, it is retried in the background (30 s, 2 min, 10 min, 30 min, 1 h).

## What this means for you (read before enabling)

1. **The model will be wrong sometimes.** In our measurements it decides in line with its teacher model in
   roughly 94% of block/allow cases on page text, and the package model is weaker (≈61% exact age bucket, 83% within one bucket).
   Wrong in the safe direction = a harmless app denied (annoyance). Wrong in the other direction = an unsuitable app allowed.
   In `quiet` only "green" can be auto-approved; in `auto` the child can also be denied things you would have allowed.
2. **First real-package probe (age 9, Arch):** `gcompris-qt`, `inkscape`, `krita` → green; `supertuxkart`, `mpv` → yellow;
   `steam`, `telegram-desktop`, `tor`, `nmap`, `hashcat` → red (good); but also `gimp`, `blender`, `kdenlive`, `wine`, `openttd` → red.
   So expect **too many denials in `auto`** until you add parent corrections; that is the safe failure.
3. **Labels came from a model, not from humans.** There is no human-checked test set yet. The fix is the rollout below
   (use `ask` first and collect your verdicts).
4. **Nobody asks you in `auto`/`quiet` for the decided items.** You find out in the evening. Anything harmful that was approved stays
   installed until you see the digest. Use `quiet` before `auto`, and keep `always_ask` current.
5. **The child should know.** Tell your child that decisions may be made automatically and that you receive a daily summary.
6. **Privacy:** everything runs on the child's machine; the sidecar has no network (systemd `PrivateNetwork=yes`), its socket is `0600` (the child
   cannot talk to it), no page/chat text is stored or sent, the digest contains package names, colours and counts only.
7. **Resources:** the sidecar runs at `Nice=19`, `CPUWeight=10`, `MemoryMax=1500M`; one rating takes about 0.2–0.3 s on CPU. Disk: ~750 MB models + ~200 MB Python libraries.
8. **Beta limits:** one child age per installation (`child_age`); mode is switched with `guardian-ctl` on the PC (not yet from the phone app).

## Not wired yet (be honest with yourself)

- **Web pages:** the sidecar can score page text (`check_page`), but nothing in the daemon calls it yet (the DNS layer remains word/domain based).
- **Grooming / chats:** the sidecar has a grooming monitor (`grooming` op), but there is no chat collector yet. The model was trained on a public 2000s English corpus (PAN-2012): per 12-message window recall 95% / FPR 3%, per whole conversation recall only 43%. Treat it as an assist.
- **Phone app:** no switch and no "undo" button for model decisions yet; the digest is text.
- **Per-child ages and per-package overrides** beyond `ask-about`.

## Install on a test machine

```bash
# 1. models (not in git, ~750 MB): copy the release folder, e.g.
scp -r laptop:Work/ocpg-models ~/ocpg-models
# 2. sidecar
sudo ai-sidecar/install.sh ~/ocpg-models
# 3. daemon 0.4.0 (cargo build --release as usual), then add to /etc/guardian/policy.toml:
#    [autonomy]
#    mode = "ask"
#    child_age = 10
sudo systemctl restart guardiand
guardian-ctl autonomy            # shows the mode
guardian-ctl autonomy quiet      # or auto
guardian-ctl digest-now          # send the digest right now to check the phone
```

Check the sidecar: `journalctl -u guardian-ai -f` (it logs op names and verdicts, never text).

**Rollback:** `guardian-ctl autonomy ask` (instant, back to the old behaviour), or `systemctl disable --now guardian-ai`.
If the sidecar is down the daemon asks the parent for everything – it never approves without a model answer.

## Recommended rollout

1. Two weeks in `ask`: look at what the model *would* have said (journal lines "AI defers …") and compare with your decisions.
2. `quiet` for a few weeks, read the evening digests.
3. `auto` only if the digests match your own judgement; keep `ask-about` for corrections.
4. Collect your corrections: they are the human labels needed to retrain (the biggest current weakness).

## Tuning

Daemon: `[autonomy]` in `policy.toml` (see `packaging/policy.example.toml`). Models: `config.json` next to the models
(`content.main_block_above`, `pkg.conservative_p`, grooming threshold). Model details, metrics and retraining: the `README.md` shipped with the models.
