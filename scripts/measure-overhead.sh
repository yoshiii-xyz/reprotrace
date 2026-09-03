#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    printf '%s\n' "usage: $0 REPROTRACE_BINARY FIXTURE_PROJECT" >&2
    exit 2
fi

reprotrace_binary=$1
fixture_project=$2
run_dir=$(mktemp -d)
printf 'run_dir=%s\n' "$run_dir"

cp -a "$fixture_project" "$run_dir/baseline"
cp -a "$fixture_project" "$run_dir/traced"

printf '%s\n' "baseline"
(
    cd "$run_dir/baseline"
    timeout --foreground 60s /usr/bin/time -f 'elapsed_seconds=%e' \
        cargo build --offline \
        >/dev/null
)

printf '%s\n' "traced"
(
    cd "$run_dir/traced"
    timeout --foreground 60s /usr/bin/time -f 'elapsed_seconds=%e' \
        "$reprotrace_binary" record \
        --output "$run_dir/traced/trace.json" \
        -- cargo build --offline \
        >/dev/null
)

printf '%s\n' "result=completed within 60 second bounds"
