#!/usr/bin/env python3
"""Generates parrot-app/assets/icon.ico and icon.png from assets/parrot.jpeg (the app logo).
Needs Pillow. Run from the workspace root: python3 scripts/generate_icon.py"""

from PIL import Image

src = Image.open("parrot-app/assets/parrot.jpeg").convert("RGBA")
src.resize((256, 256), Image.LANCZOS).save("parrot-app/assets/icon.png")
src.save(
    "parrot-app/assets/icon.ico",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
)
