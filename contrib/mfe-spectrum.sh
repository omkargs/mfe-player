#!/usr/bin/env bash
# Mini spectrum for waybar / polybar / sketchybar / YASB, fed by mfe's own
# in-process visualiser. No second cava process, no extra audio capture.
#
#   contrib/mfe-spectrum.sh [bars]     # bars defaults to 10
#
# waybar:
#   "custom/mfe-spectrum": {
#     "exec": "~/.local/bin/mfe-spectrum.sh 12",
#     "return-type": "json",
#     "interval": 1,
#     "format": "{}",
#     "on-click": "mfe toggle"
#   }
#
# YASB (Windows) and sketchybar can call the same script; both speak JSON.
set -euo pipefail

BARS="${1:-10}"
MFE="${MFE_BIN:-mfe}"

"$MFE" status --json | python3 -c '
import json, sys

bars = max(1, int(sys.argv[1]))
data = json.load(sys.stdin)
spectrum = data.get("spectrum") or []
glyphs = "▁▂▃▄▅▆▇█"

if not spectrum:
    text = "▁" * min(bars, 4)
else:
    out = []
    total = len(spectrum)
    for i in range(bars):
        low = int(i * total / bars)
        high = max(low + 1, int((i + 1) * total / bars))
        chunk = spectrum[low:high] or [0.0]
        level = sum(chunk) / len(chunk)
        out.append(glyphs[max(0, min(7, int(level * 7.999)))])
    text = "".join(out)

print(json.dumps({
    "text": text,
    "alt": data.get("alt", "idle"),
    "class": data.get("class", "stopped"),
    "tooltip": data.get("tooltip", ""),
}))
' "$BARS"
