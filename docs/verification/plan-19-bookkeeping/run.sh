#!/usr/bin/env bash
# Small end-to-end experiments for plan 19 (deterministic bookkeeping).
# Runs the real `ara` binary on throwaway copies of the agent-cli fixture
# and prints each command with its output. Usage:
#   cargo build --release && docs/verification/plan-19-bookkeeping/run.sh [path/to/ara]
set -u
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
ara=${1:-$repo/target/release/ara}
fixture=$repo/crates/ara-core/tests/fixtures/agent-cli
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fresh() {
  rm -rf "$work/a"
  cp -r "$fixture" "$work/a"
  # The fixture's historical session index predates the writer's index
  # validator, so start each experiment with an empty session history.
  rm -rf "$work/a/trace/sessions"
}

run() {
  printf '\n$ ara %s\n' "$*"
  "$ara" -C "$work/a" "$@"
  printf '[exit %s]\n' "$?"
}

apply() {
  printf '%s\n' "$@" > "$work/req.jsonl"
  printf '\n--- request ---\n'; cat "$work/req.jsonl"
  run apply "$work/req.jsonl" --json --no-duplicate-check
}

open_row() {
  "$ara" -C "$work/a" open --json | python3 -c '
import json, sys
keys = ["turns_since_reference", "session_days_since_reference", "last_reference_turn",
        "reference_basis", "history_status"]
for item in json.load(sys.stdin)["items"]:
    if item["id"] == sys.argv[1]:
        row = {k: item.get(k) for k in keys}
        row["diagnostics"] = [d["code"] for d in item.get("history_diagnostics", [])]
        print(sys.argv[1], json.dumps(row))
' "$1"
}

section() { printf '\n\n=== %s ===\n' "$1"; }

section "19a: new claim block uses schema order, inline values and lossless lists"
fresh
run claim add --title "Order probe" --set "Statement=S text" --set "Conditions=C text" \
  --set "Status=supported" --set "Falsification criteria=F text" --set 'Proof=[]' \
  --set 'Dependencies=[]' --set "Provenance=user" --set 'Tags=["x"]'
printf '\n--- tail of logic/claims.md ---\n'
tail -n 10 "$work/a/logic/claims.md"

section "19b+19c: one summarized session.log owns a batch; rows are derived"
fresh
apply \
  '{"op":"session.log","summary":"Probe ingestor tables; stage a constraint; add a claim; narrow C04"}' \
  '{"op":"node.add","type":"question","parent":"N01","title":"Does the ingestor lose table structure?","fields":{"description":"Check tables","provenance":"user"}}' \
  '{"op":"observation.stage","content":"Ingestor drops merged table cells in 3/20 PDFs","potential_type":"constraint","provenance":"ai-executed"}' \
  '{"op":"claim.add","title":"Table cells survive ingestion","fields":{"Statement":"Ingestion preserves table cells.","Conditions":"Born-digital PDFs","Falsification":"A merged cell lost after ingestion","Status":"hypothesis","Provenance":"ai-suggested"}}' \
  '{"op":"logic.revise","target":{"id":"C04"},"set":{"Status":"testing"},"signal":"empirical-resolution","provenance":"user"}'
for f in "$work"/a/trace/sessions/2*.yaml; do
  printf '\n--- trace/sessions/%s ---\n' "$(basename "$f")"
  cat "$f"
done

section "19b: omitted ownership without a session.log is refused"
fresh
apply '{"op":"logic.revise","target":{"id":"C04"},"set":{"Status":"supported"},"signal":"user-directive","provenance":"user"}'

section "19b: two open sessions on the same date make selection ambiguous"
run session start --summary "First thread"
run session start --summary "Parallel thread"
apply '' '{"op":"session.log","summary":"Which session?"}'

section "19c: a caller judgment replaces the derived revised row; contradictions reject"
fresh
apply \
  '{"op":"session.log","summary":"C04 weakened","claims_touched":[{"id":"C04","action":"refuted"}]}' \
  '{"op":"logic.revise","target":{"id":"C04"},"set":{"Status":"weakened"},"signal":"empirical-resolution","provenance":"user"}'
apply \
  '{"op":"session.log","summary":"C04 weakened","claims_touched":[{"id":"C04","action":"weakened"}]}' \
  '{"op":"logic.revise","target":{"id":"C04"},"set":{"Status":"weakened"},"signal":"empirical-resolution","provenance":"user"}'
printf '\n--- claims_touched ---\n'
grep -A4 '^claims_touched' "$work"/a/trace/sessions/2*.yaml

section "19d: open reports explicit-reference inactivity"
fresh
# The fixture's reasoning log names sessions removed by fresh(); keep it aside
# so the new observation has complete history, then restore it at the end.
mv "$work/a/trace/pm_reasoning_log.yaml" "$work/legacy_reasoning.yaml"
apply \
  '{"op":"session.log","summary":"Stage an observation","timestamp":"2026-10-01T09:00:00Z"}' \
  '{"op":"observation.stage","content":"Eval tables stay empty after the rerun","potential_type":"constraint","provenance":"user","timestamp":"2026-10-01T09:00:00Z"}'
for d in 02 03 04; do
  apply "{\"op\":\"session.log\",\"summary\":\"Unrelated work on day $d\",\"timestamp\":\"2026-10-${d}T10:00:00Z\"}"
done
printf '\n$ ara open --json   (O95 row; three unrelated turns after staging)\n'
open_row O95

section "19d: mark_stale derives session_days when they are omitted"
apply \
  '{"op":"session.log","summary":"Stale triage","timestamp":"2026-10-05T10:00:00Z"}' \
  '{"op":"observation.mark_stale","observation":"O95","reason":"No use since the rerun","audit":{"signal":"user-directive","provenance":"user"}}'
printf '\n--- stale evidence recorded for O95 ---\n'
grep -B2 -A8 'session_days' "$work/a/trace/pm_reasoning_log.yaml" | head -n 14

section "19d: a literal reference resets the counts; missing history gives null"
apply '{"op":"session.log","summary":"Re-checked O95 against the rerun","timestamp":"2026-10-06T10:00:00Z"}'
printf '\n$ ara open --json   (O95 row; after a literal reference)\n'
open_row O95
cp "$work/legacy_reasoning.yaml" "$work/a/trace/pm_reasoning_log.yaml"
printf '\n$ ara open --json   (O01 row; legacy reasoning log names missing sessions)\n'
open_row O01

section "19e: claim merge with rewrite_references repairs citers and keeps the source"
fresh
apply \
  '{"op":"session.log","summary":"Merge C09 into C01"}' \
  '{"op":"logic.revise","target":{"id":"C09"},"set":{"Status":"withdrawn","Merged into":"C01"},"signal":"user-directive","provenance":"user","rewrite_references":true}'
printf '\n--- C09, C11, C12 after the merge ---\n'
awk '/^## C(09|11|12):/ { p = 1; print; next }
     /^## / { p = 0 }
     p && /\*\*(Status|Dependencies|Merged into)\*\*/ { q = 1; print; next }
     p && q && /^  / { print; next }
     { q = 0 }' "$work/a/logic/claims.md"
printf '\n--- claims_touched ---\n'
grep -A9 '^claims_touched' "$work"/a/trace/sessions/2*.yaml
printf '\n$ ara check\n'
"$ara" check "$work/a" | tail -n 1

section "19e: a merge that would make the survivor cite itself is refused"
fresh
apply \
  '{"op":"session.log","summary":"Merge C13 into C14"}' \
  '{"op":"logic.revise","target":{"id":"C13"},"set":{"Status":"withdrawn","Merged into":"C14"},"signal":"user-directive","provenance":"user","rewrite_references":true}'
