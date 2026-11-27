"""Import installed Discord window resources. Requires fonttools, brotli and Pillow."""
import io
import json
import os
from pathlib import Path
import struct
from fontTools.ttLib import TTFont
from PIL import Image

root = Path(os.environ["LOCALAPPDATA"]) / "Discord"
release = max((p for p in root.glob("app-*") if (p/"resources/app.asar").is_file()),
              key=lambda p: tuple(map(int, p.name[4:].split("."))))
out = Path(__file__).resolve().parent / "assets"
out.mkdir(exist_ok=True)
with (release / "resources/app.asar").open("rb") as archive:
    _, header, _, length = struct.unpack("<4I", archive.read(16))
    files = json.loads(archive.read(length))["files"]["splash"]["files"]["fonts"]["files"]
    for weight, style in [(400, "normal"), (500, "medium")]:
        entry = files[f"ggsans-{weight}-{style}.woff2"]
        archive.seek(8 + header + int(entry["offset"]))
        font = TTFont(io.BytesIO(archive.read(entry["size"])), recalcTimestamp=False)
        font.flavor = None
        font.save(out / f"ggsans-{weight}.ttf")
with Image.open(root/"app.ico") as icon:
    rgba = icon.ico.getimage(max(icon.ico.sizes())).convert("RGBA").resize((64,64),Image.Resampling.LANCZOS)
    (out/"window.rgba").write_bytes(rgba.tobytes())
print(release.name, sorted(p.name for p in out.iterdir()))
