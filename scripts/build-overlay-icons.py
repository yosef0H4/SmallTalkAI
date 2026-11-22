#!/usr/bin/env python3
"""Regenerate the Linux tooltip icon font and its license with fontTools."""

from io import BytesIO
from pathlib import Path
from urllib.request import urlopen

from fontTools import subset
from fontTools.fontBuilder import FontBuilder
from fontTools.ttLib import TTFont


UPSTREAM = "https://raw.githubusercontent.com/FortAwesome/Font-Awesome/6.7.2"
FAMILY_NAME = "SmallTalkAI Overlay Icons"
FONT_NAME = "SmallTalkAIOverlayIcons-Regular"

# Keep the shared tooltip text at the same Unicode codepoints. Pango uses this
# face only on recognized icons, and Windows can keep using its native glyphs.
ICONS = {
    "🎙": 0xF130,  # microphone
    "🔴": 0xF111,  # circle
    "❌": 0xF00D,  # xmark
    "🔄": 0xF2F1,  # rotate
    "✨": 0xE2CA,  # wand-magic-sparkles
    "↪": 0xF0E2,  # arrow-rotate-left
    "⌨": 0xF11C,  # keyboard
    "🔊": 0xF028,  # volume-high
    "📷": 0xF030,  # camera
    "📋": 0xF328,  # clipboard
    "🎧": 0xF025,  # headphones
    "📖": 0xF518,  # book-open
}


def download(path):
    with urlopen(f"{UPSTREAM}/{path}", timeout=20) as source:
        return source.read()


def main():
    font = TTFont(BytesIO(download("webfonts/fa-solid-900.ttf")), recalcTimestamp=False)
    license_text = download("LICENSE.txt")
    options = subset.Options()
    options.name_IDs = [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]
    options.layout_features = []
    keep_icons = subset.Subsetter(options=options)
    keep_icons.populate(unicodes=ICONS.values())
    keep_icons.subset(font)
    unicode_to_glyph = font.getBestCmap()
    FontBuilder(font=font).setupCharacterMap(
        {ord(icon): unicode_to_glyph[codepoint] for icon, codepoint in ICONS.items()}
    )
    # The upstream OFL reserves its family name for the unmodified font.
    renamed = {
        1: FAMILY_NAME,
        2: "Regular",
        3: f"{FONT_NAME}-6.7.2",
        4: f"{FAMILY_NAME} Regular",
        6: FONT_NAME,
        16: FAMILY_NAME,
        17: "Regular",
    }
    for record in font["name"].names:
        if record.nameID in renamed:
            record.string = renamed[record.nameID]

    assets = Path(__file__).resolve().parents[1] / "assets/fonts"
    font.save(assets / f"{FONT_NAME}.ttf")
    (assets / "LICENSE-FontAwesomeFree.txt").write_bytes(license_text)


if __name__ == "__main__":
    main()
