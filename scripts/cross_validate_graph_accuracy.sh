#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
    echo "usage: $0 <first.uorobs> <second.uorobs> <output-dir>" >&2
    exit 2
fi

first=$1
second=$2
output_dir=$3
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

"$script_dir/sweep_graph_accuracy.sh" \
    "$first" \
    "$second" \
    "$output_dir/first-to-second"

"$script_dir/sweep_graph_accuracy.sh" \
    "$second" \
    "$first" \
    "$output_dir/second-to-first"
