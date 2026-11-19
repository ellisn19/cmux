"""The family's signature axis: one playful detail carried through every face.

Not chosen yet: hq docs/research/2026-11-19-our-own-type-family.md picks the axis with the bases. A spec
names it under "signature"; until a kind is written here the build refuses a spec that asks for one, so a
family never ships without the axis it declares.
"""

from __future__ import annotations

from fontTools.ttLib import TTFont

KINDS: dict = {}


def apply_signature(font: TTFont, signature: dict, face: dict) -> None:
    kind = signature.get("kind")
    if kind not in KINDS:
        raise SystemExit(f"signature kind {kind!r} is not built yet (known: {sorted(KINDS) or 'none'})")
    KINDS[kind](font, signature, face)
