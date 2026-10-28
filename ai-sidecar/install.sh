#!/usr/bin/env bash
# Install the guardian-ai sidecar (BETA). Usage: sudo ./install.sh /path/to/ocpg-models
# The models (~750 MB) are not in git; copy the release folder (content_lite, content_main_*, pkg,
# grooming, config.json) from the machine that built them, e.g.  scp -r laptop:Work/ocpg-models .
set -euo pipefail
MODELS="${1:?usage: sudo ./install.sh /path/to/ocpg-models}"
HERE="$(cd "$(dirname "$0")" && pwd)"
[ "$(id -u)" = 0 ] || { echo "run as root (sudo)"; exit 1; }
for d in content_lite content_main_xlmr pkg grooming; do [ -f "$MODELS/$d/model.int8.onnx" ] || { echo "missing $MODELS/$d/model.int8.onnx"; exit 1; }; done
[ -f "$MODELS/config.json" ] || { echo "missing $MODELS/config.json"; exit 1; }

echo "==> files"
install -d /opt/guardian-ai /usr/share/guardian-ai
install -m644 "$HERE/guardian_ai_sidecar.py" "$HERE/ocpg_ai.py" /opt/guardian-ai/
rm -rf /usr/share/guardian-ai/models && mkdir -p /usr/share/guardian-ai/models
for d in content_lite content_main_xlmr content_main_fast pkg grooming; do
  [ -d "$MODELS/$d" ] && cp -r "$MODELS/$d" /usr/share/guardian-ai/models/
done
install -m644 "$MODELS/config.json" /usr/share/guardian-ai/models/config.json
# ocpg_ai.py loads both main models lazily; if "fast" was not copied, reuse xlmr so the path exists
[ -d /usr/share/guardian-ai/models/content_main_fast ] || cp -r /usr/share/guardian-ai/models/content_main_xlmr /usr/share/guardian-ai/models/content_main_fast

echo "==> python venv (onnxruntime, tokenizers, numpy)"
python3 -m venv /opt/guardian-ai/venv
/opt/guardian-ai/venv/bin/pip install -q onnxruntime tokenizers numpy

echo "==> service"
install -Dm644 "$HERE/guardian-ai.service" /etc/systemd/system/guardian-ai.service
systemctl daemon-reload
systemctl enable --now guardian-ai.service
sleep 2
systemctl --no-pager --lines=3 status guardian-ai.service || true
cat <<MSG

Next: add an [autonomy] block to /etc/guardian/policy.toml (see packaging/policy.example.toml), restart
guardiand, then:  guardian-ctl autonomy quiet     (or auto). Read docs/AI-MODELS.md first - it is BETA.
MSG
