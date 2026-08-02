#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    echo "usage: $0 <construction.uorobs> <held-out.uorobs> <output-dir>" >&2
    exit 2
fi

construction=$1
held_out=$2
output_dir=$3
cli=${UOR_SEMANTIC_CLI:-target/release/uor-semantic}

mkdir -p "$output_dir"

for specification in \
    "r1-m64-k4 1 64 4" \
    "r3-m64-k4 3 64 4" \
    "r9-m64-k16 9 64 16" \
    "r16-m64-k16 16 64 16" \
    "r24-m64-k16 24 64 16" \
    "r24-m128-k32 24 128 32" \
    "r24-m256-k32 24 256 32" \
    "r32-m16-k1 32 16 1" \
    "r48-m16-k1 48 16 1" \
    "r48-m24-k1 48 24 1" \
    "r56-m20-k1 56 20 1" \
    "r60-m24-k1 60 24 1" \
    "r80-m32-k1 80 32 1"
do
    set -- $specification
    label=$1
    regions=$2
    overlap_margin=$3
    region_top_k=$4
    artifact="$output_dir/$label.uors"
    report="$output_dir/$label.json"

    "$cli" model compile "$construction" \
        --output "$artifact" \
        --regions "$regions" \
        --iterations 16 \
        --overlap-margin "$overlap_margin" \
        --region-top-k "$region_top_k" \
        --without-exact \
        > "$output_dir/$label.compile.txt"
    "$cli" model parity "$artifact" \
        --observations "$held_out" \
        --graph-only \
        --min-exact-bps 0 \
        --min-graph-bps 0 \
        --report "$report" \
        > "$output_dir/$label.parity.txt"
done
