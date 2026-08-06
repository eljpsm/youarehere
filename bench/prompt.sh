#!/usr/bin/env bash
#
# Prompt latency guard. Builds two throwaway repositories, times `prompt` in
# each with hyperfine, and exits nonzero when p95 crosses the limit.
#
# The prompt runs between every command a person types, so latency is a
# correctness property here. `make bench` builds the release binary first;
# running this script alone measures whatever binary is already there.
#
# Needs hyperfine, jq, and git.

set -euo pipefail

project=$(cd "$(dirname "$0")/.." && pwd)
# One temp tree for the fixtures and the hyperfine JSON, removed on any exit.
fixtures=$(mktemp -d)
trap 'rm -rf "$fixtures"' EXIT

# Identity passed with -c so the machine running this needs none configured.
# The empty commit gives the tags something to point at.
make_repo() {
    local path=$1
    git init --quiet --initial-branch=main "$path"
    git -C "$path" \
        -c user.name=Test \
        -c user.email=test@example.com \
        commit --quiet --allow-empty -m initial
}

# The common case: a repository with one tag at HEAD.
ordinary="$fixtures/ordinary"
make_repo "$ordinary"
git -C "$ordinary" tag v1

# The bad case for exact_tag, which is the only part of the prompt whose cost
# grows with the repository. Every tag points at HEAD, so git cannot discard
# any of them before sorting. One update-ref --stdin instead of 10,000 git
# processes, then pack-refs, since that is where a repository this size ends
# up on its own.
large="$fixtures/large"
make_repo "$large"
head=$(git -C "$large" rev-parse HEAD)
for index in $(seq 0 9999); do
    printf 'create refs/tags/tag-%05d %s\n' "$index" "$head"
done | git -C "$large" update-ref --stdin
git -C "$large" pack-refs --all

# Time one fixture and fail if p95 exceeds limit_ms.
run_case() {
    local name=$1
    local path=$2
    local limit_ms=$3
    local result="$fixtures/$name.json"
    local command
    # hyperfine hands this string to bash, so every path goes through %q.
    # HOME is the fixture root, so the prompt takes the tilde branch. HOSTNAME
    # is unset to time the system lookup, the slower of the two. NO_COLOR
    # keeps the work fixed regardless of the caller's environment.
    printf -v command 'cd %q && env -u HOSTNAME HOME=%q USER=bench NO_COLOR=1 %q prompt' \
        "$path" "$fixtures" "$project/target/release/youarehere"

    # The warmup runs are unmeasured, and load the binary and the refs into
    # the page cache. Without them the first run skews everything.
    hyperfine \
        --shell bash \
        --warmup 20 \
        --runs 200 \
        --export-json "$result" \
        "$command"

    # hyperfine reports mean and median, not p95. Compute it from the raw
    # times: ceil(n * 0.95) as a 1-based rank, minus 1 for the array index.
    local median
    local p95
    median=$(jq -r '.results[0].median * 1000' "$result")
    p95=$(jq -r '.results[0].times | sort | .[((length * 0.95 | ceil) - 1)] * 1000' "$result")
    printf '%s: median %.2f ms, p95 %.2f ms, limit %d ms\n' \
        "$name" "$median" "$p95" "$limit_ms"

    # The assertion. jq -e exits 1 on a false result, which set -e turns into
    # a failed bench. p95 rather than the median: a prompt that stalls one
    # keystroke in twenty is a prompt that feels slow.
    jq -e --argjson limit "$limit_ms" \
        '(.results[0].times | sort | .[((length * 0.95 | ceil) - 1)] * 1000) <= $limit' \
        "$result" >/dev/null
}

# Limits are ceilings, not targets. Both sit well above what a developer
# machine measures, so the bench catches a regression in kind (an added
# process, a walk over every ref) without failing on a loaded CI runner.
run_case ordinary "$ordinary" 10
run_case large "$large" 25
