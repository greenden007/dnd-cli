# OCR without a local model

Users who cannot run a local OCR model can use the same extraction prompt with
an image-capable hosted provider. The prompt is intentionally provider-neutral
and does not require an SDK or API key in this repository.

Generate it with:

```sh
python3 cli/scripts/build_ocr_prompt.py ./character-sheet.jpg
```

Or create an API-oriented JSON envelope:

```sh
python3 cli/scripts/build_ocr_prompt.py ./character-sheet.jpg --json
```

Attach the image to the same message as the generated prompt in ChatGPT,
Claude, DeepSeek, Kimi, Qwen, or another vision-capable provider. If a provider
has separate system and user messages, place the prompt in the system/developer
field when available and send the image plus the prompt as the user message.

The script does not upload, encode, or retain the image. Users should review the
provider's data-retention and training settings before uploading sheets that
contain personal information. The returned JSON is untrusted and must be
reviewed before importing it into the character system.
