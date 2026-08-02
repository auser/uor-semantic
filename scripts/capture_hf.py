#!/usr/bin/env python3
"""Capture deterministic next-token teacher observations from a local HF model."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


TOKENIZER_FILENAMES = {
    "added_tokens.json",
    "merges.txt",
    "sentencepiece.bpe.model",
    "spiece.model",
    "special_tokens_map.json",
    "tokenizer.json",
    "tokenizer.model",
    "tokenizer_config.json",
    "vocab.json",
    "vocab.txt",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--model-id", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--top-k", type=int, default=64)
    parser.add_argument("--max-context", type=int, default=32)
    parser.add_argument("--max-samples", type=int)
    parser.add_argument("--rollout-output", type=Path)
    parser.add_argument("--rollout-tokens", type=int, default=0)
    return parser.parse_args()


def source_tree_digest(source: Path, manifest: bytes) -> str:
    digest = hashlib.sha256()
    digest.update(manifest)
    for path in sorted(item for item in source.rglob("*") if item.is_file()):
        relative = path.relative_to(source).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        with path.open("rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
    return digest.hexdigest()


def file_set_digest(source: Path) -> str:
    digest = hashlib.sha256()
    paths = sorted(
        path
        for path in source.rglob("*")
        if path.is_file() and path.name in TOKENIZER_FILENAMES
    )
    if not paths:
        raise SystemExit("pinned source contains no recognized tokenizer files")
    for path in paths:
        relative = path.relative_to(source).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        with path.open("rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
    return digest.hexdigest()


def json_digest(value: object, label: str) -> str:
    try:
        encoded = json.dumps(
            value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
        ).encode("utf-8")
    except (TypeError, ValueError) as error:
        raise SystemExit(f"could not canonicalize tokenizer {label}: {error}") from error
    return hashlib.sha256(encoded).hexdigest()


def main() -> None:
    args = parse_args()
    if args.top_k <= 0:
        raise SystemExit("--top-k must be positive")
    if not 1 <= args.max_context <= 32:
        raise SystemExit("--max-context must be between 1 and 32")
    if args.rollout_output is not None and not 1 <= args.rollout_tokens <= 128:
        raise SystemExit("--rollout-tokens must be between 1 and 128 when rollout output is requested")
    if args.rollout_output is None and args.rollout_tokens != 0:
        raise SystemExit("--rollout-output is required when rollout-tokens is non-zero")

    try:
        import torch
        from transformers import AutoModelForCausalLM, AutoTokenizer
    except ImportError as error:
        raise SystemExit(
            "capture requires Python packages `torch` and `transformers`"
        ) from error

    torch.manual_seed(0)
    torch.set_grad_enabled(False)
    torch.use_deterministic_algorithms(True, warn_only=True)
    torch.set_num_threads(1)

    tokenizer = AutoTokenizer.from_pretrained(
        args.source, local_files_only=True, trust_remote_code=False
    )
    model = AutoModelForCausalLM.from_pretrained(
        args.source,
        local_files_only=True,
        trust_remote_code=False,
        dtype=torch.float32,
    )
    model.to("cpu")
    model.eval()

    eos_token = tokenizer.eos_token_id
    if isinstance(eos_token, (list, tuple)):
        eos_token = eos_token[0] if eos_token else None
    if eos_token is None:
        eos_token = model.config.eos_token_id
        if isinstance(eos_token, (list, tuple)):
            eos_token = eos_token[0] if eos_token else None
    if args.rollout_output is not None and eos_token is None:
        raise SystemExit("rollout capture requires a tokenizer or model eos_token_id")
    eos_token = int(eos_token) if eos_token is not None else 0
    tokenizer_sha256 = file_set_digest(args.source)
    chat_template_sha256 = json_digest(
        getattr(tokenizer, "chat_template", None), "chat template"
    )
    special_tokens_sha256 = json_digest(
        getattr(tokenizer, "special_tokens_map", {}), "special-token map"
    )

    lines = [line.rstrip("\n") for line in args.corpus.read_text(encoding="utf-8").splitlines()]
    lines = [line for line in lines if line.strip()]
    if args.max_samples is not None:
        lines = lines[: args.max_samples]
    if not lines:
        raise SystemExit("corpus contains no non-empty samples")

    manifest = "\n".join(
        [
            f"model={args.model_id}",
            f"revision={args.revision}",
            f"corpus_sha256={hashlib.sha256(args.corpus.read_bytes()).hexdigest()}",
            f"top_k={args.top_k}",
            f"max_context={args.max_context}",
            f"samples={len(lines)}",
            f"tokenizer_sha256={tokenizer_sha256}",
            f"chat_template_sha256={chat_template_sha256}",
            f"special_tokens_sha256={special_tokens_sha256}",
            f"eos_token={eos_token}",
        ]
    ).encode("utf-8")
    if args.rollout_output is not None:
        manifest += f"\nrollout_tokens={args.rollout_tokens}\neos_token={eos_token}".encode(
            "utf-8"
        )
    source_sha256 = source_tree_digest(args.source, manifest)

    records: list[str] = []
    rollout_records: list[str] = []
    vocabulary = int(model.config.vocab_size)
    retained = min(args.top_k, vocabulary)
    with torch.inference_mode():
        for text in lines:
            encoded = tokenizer(
                text,
                return_tensors="pt",
                add_special_tokens=True,
                truncation=True,
                max_length=max(args.max_context + 1, 2),
            )
            input_ids = encoded["input_ids"].to("cpu")
            if input_ids.shape[1] < 1:
                continue
            outputs = model(input_ids=input_ids, use_cache=False)
            logits = outputs.logits[0]
            tokens = input_ids[0].tolist()
            for position in range(len(tokens) - 1):
                context = tokens[max(0, position + 1 - args.max_context) : position + 1]
                values, indices = torch.topk(logits[position], k=retained)
                relative = values - values[0]
                quantized = torch.round(relative * 256.0).to(torch.int64).tolist()
                candidates = sorted(
                    zip(indices.tolist(), quantized), key=lambda item: (-item[1], item[0])
                )
                target = candidates[0][0]
                context_text = ",".join(str(token) for token in context)
                emission_text = ",".join(
                    f"{token}:{max(-(2**31), min(2**31 - 1, score))}"
                    for token, score in candidates
                )
                records.append(f"O|{context_text}|{target}|{emission_text}")

            if args.rollout_output is not None:
                prompt = tokens[-args.max_context :]
                generated: list[int] = []
                eos_position: int | None = None
                current = list(prompt)
                for step in range(args.rollout_tokens):
                    rollout_input = torch.tensor(
                        [current[-args.max_context :]], dtype=torch.long
                    )
                    rollout_outputs = model(input_ids=rollout_input, use_cache=False)
                    next_token = int(torch.argmax(rollout_outputs.logits[0, -1]).item())
                    generated.append(next_token)
                    current.append(next_token)
                    if next_token == eos_token:
                        eos_position = step
                        break
                if generated:
                    prompt_text = ",".join(str(token) for token in prompt)
                    generated_text = ",".join(str(token) for token in generated)
                    eos_text = "-" if eos_position is None else str(eos_position)
                    rollout_records.append(
                        f"R|{prompt_text}|{generated_text}|{eos_text}"
                    )

    if not records:
        raise SystemExit("capture produced no observations")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    header = [
        "UOROBS1",
        f"model={args.model_id}",
        f"revision={args.revision}",
        f"source_sha256={source_sha256}",
        f"max_context={args.max_context}",
        f"top_k={retained}",
        f"tokenizer_sha256={tokenizer_sha256}",
        f"chat_template_sha256={chat_template_sha256}",
        f"special_tokens_sha256={special_tokens_sha256}",
        f"eos_token={eos_token}",
        "--",
    ]
    args.output.write_text("\n".join(header + records) + "\n", encoding="utf-8")
    print(f"observations={len(records)}")
    print(f"source_sha256={source_sha256}")
    print(f"output={args.output}")

    if args.rollout_output is not None:
        if not rollout_records:
            raise SystemExit("rollout capture produced no sequences")
        rollout_header = [
            "UORROL1",
            f"model={args.model_id}",
            f"revision={args.revision}",
            f"source_sha256={source_sha256}",
            f"max_context={args.max_context}",
            f"max_tokens={args.rollout_tokens}",
            f"eos_token={eos_token}",
            f"tokenizer_sha256={tokenizer_sha256}",
            f"chat_template_sha256={chat_template_sha256}",
            f"special_tokens_sha256={special_tokens_sha256}",
            "--",
        ]
        args.rollout_output.parent.mkdir(parents=True, exist_ok=True)
        args.rollout_output.write_text(
            "\n".join(rollout_header + rollout_records) + "\n",
            encoding="utf-8",
        )
        print(f"rollouts={len(rollout_records)}")
        print(f"rollout_output={args.rollout_output}")


if __name__ == "__main__":
    main()
