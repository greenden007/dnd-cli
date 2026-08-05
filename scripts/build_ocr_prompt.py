#!/usr/bin/env python3
"""Build a provider-neutral OCR prompt for an attached character-sheet image.

This script never uploads the image. Attach the image to the provider manually,
then paste the generated prompt into ChatGPT, Claude, DeepSeek, Kimi, Qwen, or
another image-capable model.
"""

from __future__ import annotations

import argparse
import json
import mimetypes
from pathlib import Path


MAX_IMAGE_BYTES = 20 * 1024 * 1024
SUPPORTED_TYPES = {"image/jpeg", "image/png", "image/webp"}
TEMPLATE = Path(__file__).resolve().parents[1] / "ocr" / "character_sheet_prompt.md"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image", type=Path, help="Image to attach to the provider")
    parser.add_argument("-o", "--output", type=Path, help="Write the generated prompt to a file")
    parser.add_argument("--json", action="store_true", help="Emit a JSON envelope with attachment metadata")
    args = parser.parse_args()

    if not args.image.is_file():
        parser.error(f"image does not exist or is not a file: {args.image}")
    size = args.image.stat().st_size
    if size == 0 or size > MAX_IMAGE_BYTES:
        parser.error(f"image must be between 1 byte and {MAX_IMAGE_BYTES} bytes")

    mime_type, _ = mimetypes.guess_type(args.image.name)
    if mime_type not in SUPPORTED_TYPES:
        parser.error("image must have a .jpg, .jpeg, .png, or .webp extension")

    prompt = TEMPLATE.read_text(encoding="utf-8")
    if args.json:
        output = json.dumps(
            {
                "prompt": prompt,
                "attachment": {
                    "filename": args.image.name,
                    "mime_type": mime_type,
                    "bytes": size,
                    "instruction": "Attach this image to the same message as the prompt.",
                },
            },
            indent=2,
        )
    else:
        output = (
            f"Attach this image as the visual input: {args.image.name}\n"
            f"Detected MIME type: {mime_type}; size: {size} bytes\n\n{prompt}"
        )

    if args.output:
        args.output.write_text(output + "\n", encoding="utf-8")
    else:
        print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
