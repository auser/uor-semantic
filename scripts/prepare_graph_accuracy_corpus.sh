#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <output-dir>" >&2
    exit 2
fi

output_dir=$1
raw_dir="$output_dir/raw"
mkdir -p "$raw_dir"

train_url=https://cosmo.zip/pub/datasets/wikitext-2-raw/wiki.train.raw
valid_url=https://cosmo.zip/pub/datasets/wikitext-2-raw/wiki.valid.raw
train_path="$raw_dir/wiki.train.raw"
valid_path="$raw_dir/wiki.valid.raw"
train_sha=6707892fa3788b5ab9ed78ab5ff37d9fe825f6011a2ad4fcd6a6d467f0e7da57
valid_sha=4cd0f6876d07a413aa911261ff6d363c72d757d47f0fdd6015702014c89cb9c7

curl -sL --fail "$train_url" -o "$train_path"
curl -sL --fail "$valid_url" -o "$valid_path"

actual_train_sha=$(shasum -a 256 "$train_path" | awk '{print $1}')
actual_valid_sha=$(shasum -a 256 "$valid_path" | awk '{print $1}')
if [ "$actual_train_sha" != "$train_sha" ]; then
    echo "train split SHA-256 mismatch: $actual_train_sha" >&2
    exit 1
fi
if [ "$actual_valid_sha" != "$valid_sha" ]; then
    echo "validation split SHA-256 mismatch: $actual_valid_sha" >&2
    exit 1
fi

awk 'NF && length($0) >= 80 && (++eligible % 140) == 0 && taken < 256 { print; taken++ }' \
    "$train_path" > "$output_dir/construction.txt"
awk 'NF && length($0) >= 80 && (++eligible % 14) == 0 && taken < 256 { print; taken++ }' \
    "$valid_path" > "$output_dir/held-out.txt"

printf 'train_sha256=%s\n' "$actual_train_sha"
printf 'valid_sha256=%s\n' "$actual_valid_sha"
printf 'construction_lines=%s\n' "$(wc -l < "$output_dir/construction.txt")"
printf 'held_out_lines=%s\n' "$(wc -l < "$output_dir/held-out.txt")"
printf 'output=%s\n' "$output_dir"
