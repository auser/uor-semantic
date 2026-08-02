#!/usr/bin/env python3
"""Encode or decode token IDs with a local Hugging Face tokenizer."""

from __future__ import annotations

import argparse
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("operation", choices=["encode", "decode"])
    parser.add_argument("value")
    args = parser.parse_args()

    try:
        from transformers import AutoTokenizer
    except ImportError as error:
        raise SystemExit("tokenizer bridge requires Python package `transformers`") from error

    tokenizer = AutoTokenizer.from_pretrained(
        args.source, local_files_only=True, trust_remote_code=False
    )
    if args.operation == "encode":
        token_ids = tokenizer.encode(args.value, add_special_tokens=True)
        print(",".join(str(token) for token in token_ids))
    else:
        token_ids = [int(item) for item in args.value.split(",") if item]
        print(tokenizer.decode(token_ids, skip_special_tokens=False))


if __name__ == "__main__":
    main()
