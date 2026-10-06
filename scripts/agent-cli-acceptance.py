#!/usr/bin/env python3
"""Offline, non-model acceptance of an already-built release ara executable.

Procedure (compilation is never part of this program):
  cargo build --release --locked -p ara-cli
  python3 scripts/agent-cli-acceptance.py --binary target/release/ara \\
      --toolchain 'rustc 1.94.1 (release build)' --output /tmp/ara-agent-cli-evidence.json

Requires Python 3.10+ and an already-installed PyYAML. No dependency installation,
network access, skill subprocess, model, or experiment harness is performed.
Five fresh subprocesses per timing case; no warm-up is discarded. Each sample
starts immediately before Popen and ends after communicate consumes both pipes
and reaps the child. Generation, copying, validation oracles, and compilation
are outside the timed interval. All samples must meet the fixed budget; 100k
merge is report-only. OS caches are not flushed, and this is not a cold-cache
claim. Hardware, platform, runner label, supplied toolchain, binary SHA256,
artifact bytes/digests, complete invocations, stdout/stderr, and distributions
are included in evidence. Real merge phase clocks are read from CLI reports;
they are never inferred by subtracting other commands.

All temporary artifacts are disposable; checked-in and corpus sources are read
only. A selected output path must be outside every source tree. Omitted sections
are explicitly partial evidence, never full acceptance. Search/pair measurements
are blocked without a validated independent freeze. Pending protocol approvals
remain blockers even when other behavior passes.

Search approval JSON must name status=\"approved\", independent reviewer,
corpus_revision, relevance_sha256/pairs_sha256 (sha256:<hex>), development and
heldout objects containing recall_at_10, duplicate_precision, duplicate_recall.
An approval using crafted-only positive controls must explicitly set
allows_crafted_controls=true; that never establishes natural-pair generalization.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import resource
import shutil
import statistics
import subprocess
import re
import sys
import tempfile
import time
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
PIN = "e52a925e9d03b4ada3008653e72f99b04116fca2"
READ_SIZES = (100, 1000, 10000)
MERGE_SIZES = (100, 1000, 10000, 100000)
SECTIONS = ("reads", "corpus", "merge", "git", "replay", "coverage", "search")
TREE = "trace/exploration_tree.yaml"
INDEX = "trace/sessions/session_index.yaml"
NATIVE_PHASES = ("load_ms", "planning_ms", "validation_ms", "input_recheck_ms", "commit_ms", "advisory_ms", "operation_ms")
GIT_PHASES = ("ref_resolution_ms", "merge_base_ms", "tree_capture_ms", "blob_materialize_ms", "total_materialize_ms", "total_setup_ms")
SESSION = "2026-10-01_001"
FULL_TEXT = "Synthetic acceptance value — α\n\nLiteral @input=$binding; x = y.\n```text\n## not a heading\n```\n"


class AcceptanceFailure(Exception):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AcceptanceFailure(message)


def sha(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def dump(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def save(root: Path, relative: str, content: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content.encode("utf-8"))


def yaml_load(path: Path) -> Any:
    import yaml  # Offline prerequisite; never installed by the runner.
    # YAML 1.2 has no implicit date type and does not turn "on"/"off" into
    # booleans. The Rust source parser follows that dialect.
    class SourceLoader(yaml.SafeLoader):
        pass
    SourceLoader.yaml_implicit_resolvers = {
        initial: [(tag, pattern) for tag, pattern in resolvers
                  if tag not in ("tag:yaml.org,2002:timestamp", "tag:yaml.org,2002:bool")]
        for initial, resolvers in yaml.SafeLoader.yaml_implicit_resolvers.items()}
    SourceLoader.add_implicit_resolver("tag:yaml.org,2002:bool",
        re.compile(r"^(?:true|false|True|False|TRUE|FALSE)$"), list("tTfF"))
    try:
        return yaml.load(path.read_bytes(), Loader=SourceLoader)
    except yaml.YAMLError as error:
        raise AcceptanceFailure(f"Cannot parse exact YAML source {path}: {error}") from error


def source_files(root: Path) -> dict[str, bytes]:
    files = {}
    for path in sorted(root.rglob("*")):
        if any(part in (".ara", ".git") for part in path.relative_to(root).parts):
            continue
        if path.is_symlink():
            files[path.relative_to(root).as_posix()] = os.fsencode(os.readlink(path))
        elif path.is_file():
            files[path.relative_to(root).as_posix()] = path.read_bytes()
    return files


def artifact_info(root: Path) -> dict[str, Any]:
    files = source_files(root)
    digest = hashlib.sha256()
    for path, content in files.items():
        name = path.encode()
        digest.update(len(name).to_bytes(8, "big")); digest.update(name)
        digest.update(len(content).to_bytes(8, "big")); digest.update(content)
    return {"path": str(root), "bytes": sum(map(len, files.values())),
            "files": len(files), "inventory_sha256": digest.hexdigest(),
            "knowledge_bytes": sum(len(b) for p, b in files.items()
                                   if p == "PAPER.md" or p.startswith(("logic/", "trace/", "staging/")))}


def node_source(root: Path) -> tuple[list[dict[str, Any]], dict[str, str | None]]:
    document = yaml_load(root / TREE)
    roots = document if isinstance(document, list) else document.get("tree", document.get("root", []))
    if isinstance(roots, dict):
        roots = [roots]
    require(isinstance(roots, list), "Unsupported tree root dialect; cannot produce a faithful oracle")
    rows, parents = [], {}
    stack = [(node, None) for node in reversed(roots)]
    while stack:
        node, parent = stack.pop()
        require(isinstance(node, dict) and isinstance(node.get("id"), str), "Malformed source node")
        require(node["id"] not in parents, "Duplicate source ID " + node["id"])
        explicit = node.get("parent")
        require(explicit is None or isinstance(explicit, str), "Malformed explicit parent for " + node["id"])
        require(parent is None or explicit is None or explicit == parent,
                "Source nesting disagrees with explicit parent for " + node["id"])
        rows.append({k: v for k, v in node.items() if k != "children"})
        parents[node["id"]] = parent if parent is not None else explicit
        children = node.get("children") or []
        require(isinstance(children, list), "Malformed children for " + node["id"])
        stack.extend((child, node["id"]) for child in reversed(children))
    settled: set[str] = set()
    for identity in parents:
        visiting: set[str] = set()
        while identity and identity not in settled:
            require(identity in parents, "Unknown source parent " + identity)
            require(identity not in visiting, "Source parent cycle through " + identity)
            visiting.add(identity)
            identity = parents[identity]
        settled.update(visiting)
    return rows, parents


def node_and_staging_kind_counts(root: Path, nodes: list[dict[str, Any]]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for node in nodes:
        kind = node["type"]
        counts[kind] = counts.get(kind, 0) + 1
    path = root / "staging/observations.yaml"
    if path.exists():
        rows = yaml_load(path)["observations"]
        require(isinstance(rows, list) and all(isinstance(row, dict) and isinstance(row.get("id"), str) and isinstance(row.get("content"), str) for row in rows),
                "Pinned status oracle requires complete source staging records")
        counts["observation"] = counts.get("observation", 0) + len(rows)
    return counts


def replay_incompatibilities(nodes: list[dict[str, Any]], parents: dict[str, str | None]) -> list[dict[str, Any]]:
    """Report all unchanged historical source incompatibilities before attempting replay."""
    required = {"question": ("description",), "decision": ("choice", "alternatives"),
                "experiment": ("result",), "dead_end": ("hypothesis", "failure_mode", "lesson"),
                "pivot": ("from", "to", "trigger")}
    issues = []
    for node in nodes:
        kind = node.get("type")
        reasons = []
        if kind not in required:
            reasons.append("unsupported node.add type: " + str(kind))
        else:
            missing = [field for field in required[kind] if field not in node]
            if missing:
                reasons.append("missing required unchanged source payload: " + ", ".join(missing))
        children = [identity for identity, parent in parents.items() if parent == node["id"]]
        if kind == "dead_end" and children:
            reasons.append("dead_end is a leaf in node.add; original children: " + ", ".join(children))
        ancestor = parents[node["id"]]
        ancestors = set()
        while ancestor:
            ancestors.add(ancestor)
            ancestor = parents[ancestor]
        redundant = sorted(set(node.get("also_depends_on", [])) & ancestors)
        if redundant:
            reasons.append("strict node.add rejects recorded ancestor dependencies: " + ", ".join(redundant))
        if reasons:
            issues.append({"id": node["id"], "source_node": node, "children": children, "reasons": reasons})
    return issues


def replay_sequence(nodes: list[dict[str, Any]], parents: dict[str, str | None],
                    recorded: list[str]) -> tuple[list[str], list[dict[str, Any]]]:
    """Preserve recorded order; infer only missing structural prerequisite seeds."""
    by_id = {node["id"]: node for node in nodes}
    recorded_ids, emitted, visiting = set(recorded), set(), set()
    order, inferences = [], []

    def emit(identity: str, dependent: str | None = None) -> None:
        if identity in emitted:
            return
        require(identity in by_id, "Replay references an absent source node " + identity)
        require(identity not in visiting, "Replay source prerequisite cycle at " + identity)
        require(dependent is None or identity not in recorded_ids,
                f"Recorded event order contradicts prerequisite {identity} before {dependent}; recorded events are not reordered")
        node = by_id[identity]
        if dependent is not None:
            earlier = str(node.get("timestamp", ""))[:10]
            later = str(by_id[dependent].get("timestamp", ""))[:10]
            require(not earlier or not later or earlier <= later,
                    f"Observed source timestamps contradict prerequisite {identity} ({earlier}) before {dependent} ({later})")
        visiting.add(identity)
        prerequisites = [parents[identity], node.get("parent"), *node.get("also_depends_on", [])]
        for prerequisite in dict.fromkeys(prerequisites):
            if prerequisite and prerequisite != "root" and prerequisite not in emitted:
                emit(prerequisite, identity)
        visiting.remove(identity)
        emitted.add(identity); order.append(identity)
        if identity not in recorded_ids:
            inferences.append({"id": identity, "status": "[INFERENCE]",
                "reason": "missing original session-event provenance; required prerequisite seed" if dependent else "unlogged tail in deterministic source prerequisite order",
                "required_before": dependent, "observed_timestamp": node.get("timestamp"),
                "claim": "structural prerequisites only; not reconstructed historical session chronology"})

    for identity in recorded:
        emit(identity)
    for node in nodes:
        if node["id"] not in emitted:
            emit(node["id"])
    require([identity for identity in order if identity in recorded_ids] == recorded,
            "Replay changed recorded session/event order")
    return order, inferences


def node_fields(kind: str, marker: str) -> dict[str, Any]:
    common = {"timestamp": "2026-10-01T10:00", "provenance": "ai-executed"}
    values = {
        "question": {"description": marker},
        "decision": {"choice": marker, "alternatives": ["synthetic alternative"], "evidence": "Synthetic source input; no empirical claim"},
        "experiment": {"result": marker, "evidence": ["C01"]},
        "dead_end": {"hypothesis": marker, "failure_mode": "Synthetic boundary", "lesson": "Keep the supplied failure record"},
        "pivot": {"from": "Synthetic initial direction", "to": marker, "trigger": "Synthetic user directive"},
    }
    return common | values[kind]


def claim_fields(marker: str = FULL_TEXT) -> dict[str, Any]:
    return {"Statement": marker, "Conditions": "Synthetic fixture only; not a scientific claim",
            "Sources": ["1 ← acceptance-input.json «\"count\":1» [input]"],
            "Status": "hypothesis", "Provenance": "user", "Falsification": "A differing decoded source value disproves exact retention",
            "Proof": ["pending"], "Dependencies": [], "Tags": ["synthetic"]}


def heuristic_fields(marker: str = FULL_TEXT) -> dict[str, Any]:
    return {"Rationale": marker, "Sources": ["acceptance-input.json «synthetic» [input]"],
            "Status": "active", "Provenance": "user", "Sensitivity": "unknown", "Code ref": ["pending"]}

def compiler_heuristic_fields(marker: str = FULL_TEXT) -> dict[str, Any]:
    return {"Rationale": marker,
            "Source": "acceptance-input.json «synthetic = α; [input], unchanged»\n  exact source continuation\n",
            "Sensitivity": "Not specified in paper",
            "Bounds": "Synthetic fixture only; no empirical generalization.\n  Caller-selected boundary = α\n",
            "Code ref": "src/untouched.txt:1 = α; [literal], not a list\n  exact code continuation\n"}



def markdown_entry(identity: str, title: str, fields: dict[str, Any]) -> str:
    text = f"## {identity}: {title}\n"
    for label, value in fields.items():
        value = value if isinstance(value, str) else dump(value)
        text += f"- **{label}**:\n" + "".join("  " + line + "\n" for line in value.split("\n"))
    return text + "\n"


def input_documents() -> dict[str, str]:
    return {
        "PAPER.md": "---\ntitle: Synthetic deterministic acceptance\nauthors: [Acceptance fixture]\nextension: {opaque: [α, null, 3]}\n---\n# Synthetic Artifact\n\nNo measured research result.\n\n## Layer Index\n\nCaller-selected layer index.\n",
        "logic/problem.md": "# Problem\n\n## Assumptions\n\n" + FULL_TEXT,
        "logic/claims.md": "# Claims\n\n" + markdown_entry("C01", "Synthetic exact-retention claim", claim_fields()),
        "logic/concepts.md": "# Concepts\n\n## Synthetic term\n\n- **Definition**:\n  Exact fixture definition.\n- **Provenance**:\n  user\n",
        "logic/experiments.md": "# Experiments\n\n## E01: Synthetic plan\n\n- **Question**: Does source retention hold?\n- **Setup**: Caller supplies complete UTF-8.\n- **Status**: planned\n- **Evidence output**: pending\n",
        "logic/related_work.md": "# Related Work\n\n## RW01: Synthetic comparison\n\n- **DOI**: pending\n- **Delta**: Exact source retention only.\n",
        "logic/solution/constraints.md": "# Constraints\n\n## Source boundary\n\n" + FULL_TEXT,
        "logic/solution/heuristics.md": "# Heuristics\n\n" + markdown_entry("H01", "Synthetic retention heuristic", compiler_heuristic_fields()),
    }


def generate_artifact(root: Path, count: int, shape: str = "broad", start: int = 1,
                      layer_start: int = 1, marker: str = "base") -> dict[str, Any]:
    """Linear-byte broad/deep source; deep uses nested flow YAML, not huge indentation.

    Layers scale with nodes (one entry per 20 nodes); session rows scale equally,
    grouped in batches of 100. No references target absent nodes or parent edges.
    Identity starts let a merge trio use true shared base plus disjoint fork data.
    """
    require(count > 0 and shape in ("broad", "deep"), "Invalid generator parameters")
    root.mkdir(parents=True, exist_ok=True)
    node_ids = [f"N{i:02}" for i in range(start, start + count)]
    layer_count = max(1, count // 20)
    layer_ids = list(range(layer_start, layer_start + layer_count))
    pieces = []
    kinds = ("question", "decision", "experiment", "dead_end", "pivot")
    deep_kinds = ("question", "decision", "experiment", "pivot")
    def kind_at(index: int) -> str:
        if shape == "deep" and count > 1 and index == count - 1:
            return "dead_end"
        return (deep_kinds if shape == "deep" else kinds)[index % (len(deep_kinds) if shape == "deep" else len(kinds))]
    for index, identity in enumerate(node_ids):
        kind = kind_at(index)
        fields = node_fields(kind, f"Synthetic {marker} payload {identity}")
        if kind == "experiment":
            fields["evidence"] = [f"C{layer_ids[index % len(layer_ids)]:02}"]
        node = {"id": identity, "type": kind, "title": f"Synthetic {marker} {identity}" + (" uniqueneedle" if index == count - 1 else ""), **fields}
        if index > 1 and shape == "broad":
            node["also_depends_on"] = [node_ids[0]]
        serialized = dump(node)
        pieces.append(serialized[:-1] + ',"children":[' if shape == "deep" and index < count - 1 else serialized)
    tree = '{"tree":[' + ("".join(pieces) + "]}" * (count - 1) if shape == "deep" else ",".join(pieces)) + "]}\n"
    save(root, TREE, tree)
    save(root, "PAPER.md", input_documents()["PAPER.md"])
    claims, heuristics, plans, concepts, related, observations, tastes, reasoning = [], [], [], [], [], [], [], []
    index_rows = []
    for offset, number in enumerate(layer_ids):
        nid = node_ids[offset % len(node_ids)]
        cid, hid, eid, oid, tid = (f"{prefix}{number:02}" for prefix in "CHEOT")
        claims.append(markdown_entry(cid, f"Synthetic {marker} claim", claim_fields(f"Synthetic {marker} statement {cid}; node {nid}")))
        heuristics.append(markdown_entry(hid, f"Synthetic {marker} heuristic", heuristic_fields(f"Synthetic {marker} rationale {hid}")))
        plans.append(markdown_entry(eid, f"Synthetic {marker} plan", {"Setup": f"Synthetic {nid}", "Question": "Exact source retention", "Status": "planned", "Evidence output": "pending"}))
        concepts.append(f"## Synthetic {marker} term {number}\n\n- **Definition**: Exact {nid} fixture definition.\n\n")
        related.append(f"## RW{number:02}: Synthetic {marker} comparison\n\n- **Delta**: Reference {cid}.\n\n")
        observations.append({"id": oid, "timestamp": "2026-10-01T09:00", "provenance": "ai-suggested", "content": f"Synthetic {marker} observation {oid}", "context": f"Caller supplied {nid}", "potential_type": "claim", "bound_to": [nid], "promoted": False, "promoted_to": None, "crystallized_via": None, "stale": False})
        # Taste targets never point at a question, including in deep shapes.
        if count > 1:
            tastes.append({"id": tid, "timestamp": "2026-10-01T10:00", "target": node_ids[1], "tag": "uncertain", "object": "framing", "comment": f"Synthetic {marker} reaction {tid}"})
    for group in range(math.ceil(layer_count / 100)):
        # Date/sequence is disjoint by the explicit layer start, but deterministic.
        date = (dt.date(2026, 10, 1) + dt.timedelta(days=layer_start + group)).isoformat()
        sid = date + "_001"
        subset = list(enumerate(layer_ids))[group * 100:(group + 1) * 100]
        events, actions, touched, revisions, contexts = [], [], [], [], []
        for turn, (offset, number) in enumerate(subset, 1):
            nid, cid = node_ids[offset % len(node_ids)], f"C{number:02}"
            event_kind = kind_at(offset % count)
            events.append({"turn": turn, "type": event_kind, "id": nid, "routing": "direct", "provenance": "ai-executed", "summary": f"Synthetic {marker} event {turn}"})
            actions.append({"turn": turn, "action": f"Synthetic {marker} action {turn}", "provenance": "ai-executed", "files_changed": ["logic/claims.md"]})
            touched.append({"turn": turn, "id": cid, "action": "created"})
            revisions.append({"turn": turn, "entry": cid, "field": "Statement", "before": "", "after": f"Synthetic {marker} statement {cid}", "signal": "user-directive", "provenance": "user", "note": "Synthetic history row"})
            contexts.append({"turn": turn, "excerpt": f"Synthetic {marker} decisive context {turn}"})
            reasoning.append({"turn": f"{sid}#{turn}", "notes": [f"Synthetic {marker} rejected near-miss"]})
        session = {"session": {"id": sid, "date": date, "started": date + "T10:00", "last_turn": date + "T11:00", "turn_count": len(subset), "summary": f"Synthetic {marker} session"}, "events_logged": events, "ai_actions": actions, "claims_touched": touched, "logic_revisions": revisions, "key_context": contexts, "open_threads": ["Synthetic remaining question"], "ai_suggestions_pending": ["Synthetic unconfirmed suggestion"]}
        save(root, f"trace/sessions/{sid}.yaml", dump(session) + "\n")
        index_rows.append({"id": sid, "date": date, "summary": session["session"]["summary"], "turn_count": len(subset), "events_count": len(events), "claims_touched": [row["id"] for row in touched], "open_threads": 1})
    for path, heading, entries in [("logic/claims.md", "Claims", claims), ("logic/solution/heuristics.md", "Heuristics", heuristics), ("logic/experiments.md", "Experiments", plans), ("logic/concepts.md", "Concepts", concepts), ("logic/related_work.md", "Related Work", related)]:
        save(root, path, "# " + heading + "\n\n" + "".join(entries))
    for path in ("logic/problem.md", "logic/solution/constraints.md", "logic/solution/architecture.md", "logic/solution/algorithm.md"):
        save(root, path, f"# Synthetic {marker} {path}\n\n" + FULL_TEXT)
    for path, key, value in [("staging/observations.yaml", "observations", observations), ("trace/taste_log.yaml", "entries", tastes), ("trace/pm_reasoning_log.yaml", "entries", reasoning), (INDEX, "sessions", index_rows)]:
        save(root, path, dump({key: value}) + "\n")
    save(root, "src/untouched.txt", "Synthetic external source body; never CLI edited.\n")
    save(root, "evidence/untouched.txt", "Synthetic raw evidence; never CLI edited.\n")
    return {"nodes": count, "shape": shape, "layer_entries_each": layer_count, "session_rows_each": layer_count, "sessions": len(index_rows), "node_ids": node_ids, "claim": f"C{layer_start:02}"}


def merge_trio(root: Path, total: int) -> tuple[Path, Path, Path, dict[str, Any]]:
    base_count, append_count = total * 8 // 10, total // 10
    base, ours, theirs = (root / name for name in ("base", "ours", "theirs"))
    meta = generate_artifact(base, base_count)
    shared_tree = json.loads((base / TREE).read_text())
    # Reserve N03 as the independent cross-edge target. If it depends on N01,
    # the directional union N01 -> ours -> N02 -> theirs -> N03 -> N01 cycles.
    shared_tree["tree"][2].pop("also_depends_on", None)
    save(base, TREE, dump(shared_tree) + "\n")
    shutil.copytree(base, ours); shutil.copytree(base, theirs)
    for branch, marker in ((ours, "ours"), (theirs, "theirs")):
        additions = root / (marker + "-append")
        added = generate_artifact(additions, append_count, start=base_count + 1,
                                  layer_start=meta["layer_entries_each"] + 1, marker=marker)
        tree = json.loads((branch / TREE).read_text())
        incoming = json.loads((additions / TREE).read_text())
        # Fork numeric identities collide, but their structural parents differ.
        for index, node in enumerate(incoming["tree"]):
            node["acceptance_extension"] = {"marker": marker, "opaque": ["α", None, {"nested": index + 1}]}
            if index > 1:
                node["also_depends_on"] = [f"N{2 if marker == 'ours' else 3:02}"]
        tree["tree"][0 if marker == "ours" else 1].setdefault("children", []).extend(incoming["tree"])
        save(branch, TREE, dump(tree) + "\n")
        for path, content in source_files(additions).items():
            if path == TREE or path == "PAPER.md" or path.startswith(("src/", "evidence/")):
                continue
            if path in ("staging/observations.yaml", "trace/taste_log.yaml", "trace/pm_reasoning_log.yaml", INDEX):
                old, new = json.loads((branch / path).read_text()), json.loads(content)
                key = next(iter(old)); old[key].extend(new[key])
                save(branch, path, dump(old) + "\n")
            elif path.startswith("trace/sessions/"):
                # A real independent-session collision, not a row-equality shortcut.
                save(branch, path, content.decode())
            elif path in ("logic/claims.md", "logic/solution/heuristics.md", "logic/experiments.md", "logic/concepts.md", "logic/related_work.md"):
                body = content.decode().split("\n\n", 1)[1]
                if path == "logic/related_work.md" and marker == "theirs":
                    # RW uses native nonnumeric identity, so clean generators
                    # must not disguise two different same-address items.
                    for number in reversed(range(meta["layer_entries_each"] + 1,
                                                  meta["layer_entries_each"] + added["layer_entries_each"] + 1)):
                        body = body.replace(f"## RW{number:02}:", f"## RW{number + added['layer_entries_each']:02}:")
                save(branch, path, (branch / path).read_text() + body)
        # Distinct whole mutable bodies deliberately create full-evidence conflicts.
        # For timed clean merges only theirs changes these bodies; ours keeps base.
        if marker == "theirs":
            for path in ("logic/problem.md", "logic/solution/constraints.md", "logic/solution/architecture.md", "logic/solution/algorithm.md"):
                save(branch, path, (additions / path).read_text())
    return base, ours, theirs, {"merged_nodes": total, "base_nodes": base_count,
                                "fork_append_nodes_each": append_count, "layers": added}

def comparable_merge_report(value: dict[str, Any]) -> dict[str, Any]:
    """Keep every decision; exclude only top-level transport and clock reports."""
    return {key: item for key, item in value.items() if key not in ("git", "git_timings", "timings")}


def comparable_merge_journal(value: dict[str, Any]) -> dict[str, Any]:
    """Normalize typed clock/provenance slots, never opaque source/conflict data."""
    require(value.get("format") == "ara.merge-log/v1", "Unsupported portable merge journal")
    result = {**value, "records": []}
    for original in value["records"]:
        record = dict(original)
        if record["kind"] in ("enrollment", "label", "revision", "resolution"):
            require(isinstance(record.get("time"), str), "Typed journal record lacks captured time")
            record["time"] = "<captured runtime time>"
        if record["kind"] == "revision":
            record.pop("git", None)
        result["records"].append(record)
    return result


def git_state(repository: Path) -> dict[str, Any]:
    """Snapshot worktree outside ara and exact control-file existence/content."""
    state: dict[str, Any] = {}
    for child in repository.iterdir():
        if child.name in (".git", "ara"):
            continue
        files = source_files(child) if child.is_dir() else {"": child.read_bytes()}
        for path, content in files.items():
            state["outside/" + child.name + ("/" + path if path else "")] = {"bytes": len(content), "sha256": sha(content)}
    control = repository / ".git"
    for name in ("HEAD", "index", "packed-refs", "config", "MERGE_HEAD", "MERGE_MSG", "MERGE_MODE",
                 "AUTO_MERGE", "ORIG_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "REBASE_HEAD"):
        path = control / name
        state[".git/" + name] = {"bytes": path.stat().st_size, "sha256": sha(path.read_bytes())} if path.is_file() else None
    for name in ("refs", "logs"):
        path = control / name
        if path.is_dir():
            for relative, content in source_files(path).items():
                state[".git/" + name + "/" + relative] = {"bytes": len(content), "sha256": sha(content)}
    return state


class Runner:
    def __init__(self, args: argparse.Namespace, workspace: Path):
        self.args, self.workspace = args, workspace
        self.evidence: dict[str, Any] = {"format": "ara.agent-cli-acceptance/v1", "scientific_claim": False,
            "model_experiments": "deferred; none run", "sections": {}, "invocations": [], "failures": [], "blockers": [],
            "procedure": {"samples": args.repeats, "discarded_warmups": 0, "clock": "perf_counter_ns",
                "interval": "immediately before Popen through communicate and wait, consuming stdout and stderr",
                "compile_included": False, "caches": "OS caches uncontrolled; no cold-cache claim",
                "read_real_limit_ms": 100, "read_generated_limit_ms": 1000,
                "merge_10000_limit_ms": 1000, "merge_100000": "report-only",
                "phase_measurements": "observed CLI load/planning/validation/input-recheck/commit/advisory/operation clocks; process interval remains authoritative",
                "session_date": "2026-10-01", "sections_requested": args.sections},
            "environment": {"runner": args.runner, "platform": platform.platform(), "machine": platform.machine(),
                "processor": platform.processor(), "cpu_count": os.cpu_count(), "python": sys.version,
                "toolchain": args.toolchain, "binary": str(args.binary), "binary_sha256": sha(args.binary.read_bytes())}}
        self.rows: dict[str, dict[str, Any]] = {}

    def command(self, root: Path, words: list[str], expected: tuple[int, ...] = (0,),
                input_bytes: bytes | None = None, timeout: float | None = None,
                cwd: Path | None = None, explicit_root: bool = True) -> tuple[dict[str, Any], int]:
        argv = [str(self.args.binary)] + (["-C", str(root)] if explicit_root else []) + words
        environment = os.environ.copy()
        environment.pop("ARA_DIR", None)
        environment.pop("ARA_NO_DUPLICATE_CHECK", None)
        start = time.perf_counter_ns()
        process = subprocess.Popen(argv, cwd=cwd or ROOT, env=environment, stdin=subprocess.PIPE if input_bytes is not None else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        timed_out = False
        try:
            stdout, stderr = process.communicate(input_bytes, timeout=timeout or self.args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            process.kill(); stdout, stderr = process.communicate()
        elapsed = (time.perf_counter_ns() - start) / 1e6
        record = {"argv": argv, "cwd": str(cwd or ROOT), "exit": process.returncode, "elapsed_ms": elapsed,
                  "stdout_bytes": len(stdout), "stderr_bytes": len(stderr), "timed_out": timed_out,
                  "stdout": stdout.decode("utf-8", "replace"), "stderr": stderr.decode("utf-8", "replace")}
        index = len(self.evidence["invocations"]); self.evidence["invocations"].append(record)
        if input_bytes is not None:
            record["stdin"] = input_bytes.decode("utf-8")
            record["stdin_sha256"] = sha(input_bytes)
        require(not timed_out, f"Timeout after {timeout or self.args.timeout}s: {argv}")
        require(process.returncode in expected, f"Expected exit {expected}, observed {process.returncode}: {argv}; {record['stderr']}")
        try:
            # Status uniquely returns useful success-envelope diagnostics on exit 1.
            if process.returncode == 0 or words[0] == "status" and stdout:
                require(not stderr, "Success JSON leaked text to stderr")
                value = json.loads(stdout)
                require(isinstance(value, dict), "Success is not one JSON object")
                if words[0] not in ("validate", "check"):
                    require(isinstance(value.get("format"), str), "Agent success lacks versioned format")
            else:
                require(not stdout, "Failure leaked output to stdout")
                value = json.loads(stderr)
                require(isinstance(value, dict) and isinstance(value.get("error"), dict), "Failure lacks structured error")
                require(isinstance(value["error"].get("code"), str), "Failure lacks explicit error code")
        except (ValueError, UnicodeError) as error:
            raise AcceptanceFailure(f"Unparseable command JSON: {argv}: {error}") from error
        record["result"] = value
        return value, index

    def apply(self, root: Path, operations: list[dict[str, Any]], dry: bool = False,
              expected: tuple[int, ...] = (0,)) -> dict[str, Any]:
        value, _ = self.command(root, ["apply", "-", *( ["--dry-run"] if dry else []), "--json"], expected,
                                ("\n".join(map(dump, operations)) + "\n").encode())
        if expected == (0,):
            require(value.get("committed") is (not dry), "apply committed flag disagrees with requested mode")
        return value

    def source(self, root: Path, document: str, headings: tuple[str, ...] = ()) -> str:
        words = ["show", "--document", document, "--source", "--full", "--json"]
        for heading in headings:
            words.extend(["--heading", heading])
        value, _ = self.command(root, words)
        entries = value.get("entries", [])
        require(len(entries) == 1, "source read did not resolve exactly one document")
        entry = entries[0]
        require(isinstance(entry.get("content"), str), "source read omitted exact content")
        require(entry.get("digest") == sha(entry["content"].encode()), "source digest does not identify returned bytes")
        if not headings:
            require(entry["content"].encode() == (root / document).read_bytes(), "source retrieval truncated or changed document " + document)
        return entry["content"]

    def scenario(self, name: str, work: Callable[[], Any]) -> None:
        begin = len(self.evidence["invocations"])
        try:
            result = work()
            self.evidence["sections"][name] = {"passed": True, "data": result, "invocations": list(range(begin, len(self.evidence["invocations"])))}
        except (AcceptanceFailure, OSError, ValueError, KeyError, TypeError, RecursionError, StopIteration) as error:
            failure = {"scenario": name, "message": str(error), "exception": type(error).__name__}
            self.evidence["failures"].append(failure)
            self.evidence["sections"][name] = {"passed": False, "failure": failure, "invocations": list(range(begin, len(self.evidence["invocations"])))}

    def prove(self, ids: list[str], name: str, work: Callable[[], Any]) -> None:
        self.scenario("coverage." + name, work)
        proof = self.evidence["sections"]["coverage." + name]
        for identity in ids:
            require(identity in self.rows, "Scenario invented an inventory row " + identity)
            self.rows[identity]["proofs"].append({"scenario": name, **proof})

    def timing(self, root: Path, words: list[str], budget: float | None,
               oracle: Callable[[dict[str, Any]], None] | None = None,
               expected: tuple[int, ...] = (0,), prepare: Callable[[], Path] | None = None) -> dict[str, Any]:
        samples, indices, phases = [], [], {}
        for _ in range(self.args.repeats):
            target = prepare() if prepare else root
            value, index = self.command(target, words, expected, timeout=120 if budget is None else self.args.timeout)
            if oracle:
                oracle(value)
            samples.append(self.evidence["invocations"][index]["elapsed_ms"]); indices.append(index)
            if words[0] == "merge":
                groups = [("timings", "", NATIVE_PHASES)]
                if "--git" in words:
                    groups.append(("git_timings", "git.", GIT_PHASES))
                for namespace, prefix, keys in groups:
                    observed = value.get(namespace)
                    require(isinstance(observed, dict), "Merge report lacks actual clocks: " + namespace)
                    for key in keys:
                        elapsed = observed.get(key)
                        require(isinstance(elapsed, (int, float)) and not isinstance(elapsed, bool) and math.isfinite(elapsed) and elapsed >= 0,
                                "Merge phase clock is missing or invalid: " + namespace + "." + key)
                        phases.setdefault(prefix + key, []).append(elapsed)
        ordered = sorted(samples)
        distribution = {"samples_ms": samples, "min_ms": min(samples), "median_ms": statistics.median(samples),
                        "mean_ms": statistics.mean(samples), "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1], "max_ms": max(samples),
                        "budget_ms": budget, "invocations": indices}
        if phases:
            distribution["observed_cli_phases"] = {
                key: {"samples_ms": values, "min_ms": min(values), "median_ms": statistics.median(values),
                      "mean_ms": statistics.mean(values), "p95_ms": sorted(values)[math.ceil(len(values) * .95) - 1], "max_ms": max(values)}
                for key, values in phases.items()}
            distribution["phase_interval_limit"] = "Native operation_ms ends before output serialization; Git tree/blob cumulative spans overlap with pipeline backpressure. Observed stages are not inferred or additive; process wall time is authoritative."
        # Retain failed measurements in machine evidence before raising.
        self.evidence.setdefault("timings", []).append({"artifact": artifact_info(root), "command": words, **distribution})
        if budget is not None:
            require(all(sample < budget for sample in samples), f"Timing budget {budget}ms missed: {words}: {samples}")
        return distribution

    def reads(self) -> dict[str, Any]:
        fixture = self.args.fixture
        nodes, parents = node_source(fixture)
        by_id = {node["id"]: node for node in nodes}
        require(all(identity in by_id for identity in ("N12", "N62", "N85")), "Pinned read selectors absent")
        require("## C05:" in (fixture / "logic/claims.md").read_text(), "Pinned C05 selector absent")
        def show_oracle(value: dict[str, Any]) -> None:
            entries = value["entries"]; require([entry["id"] for entry in entries] == ["N62"], "Real show identity mismatch")
            require(entries[0]["title"] == by_id["N62"]["title"], "Real show title mismatch")
            expected = [parents["N62"]] if parents["N62"] else []
            require(entries[0]["relations"]["parents"] == expected, "Real show parent relation mismatch")
            require(entries[0]["relations"]["children"] == [identity for identity, parent in parents.items() if parent == "N62"], "Real show children mismatch")
            evidence = by_id["N62"].get("evidence", [])
            claim_ids = [identity for identity in evidence if isinstance(identity, str) and re.fullmatch(r"C[0-9]+", identity)] if isinstance(evidence, list) else []
            require(entries[0]["relations"]["claims"] == claim_ids, "Real show claim bindings mismatch")
        def status_oracle(value: dict[str, Any]) -> None:
            require(value["complete"] is True, "Pinned fixture status is explicitly incomplete")
            for kind, expected in node_and_staging_kind_counts(fixture, nodes).items():
                require(value["counts"].get(kind, 0) == expected,
                        "Real status source node/staging kind count mismatch for " + kind)
            require(value["next_ids"]["N"] == f"N{max(int(identity[1:]) for identity in by_id) + 1:02}",
                    "Real status next-ID advice differs from maximum observed source identity")
        def list_oracle(value: dict[str, Any]) -> None:
            expected = []
            for identity, node in by_id.items():
                ancestor = parents[identity]
                while ancestor and ancestor != "N12":
                    ancestor = parents[ancestor]
                if ancestor == "N12" and node["type"] == "dead_end":
                    expected.append(identity)
            require([row["id"] for row in value["entries"]] == expected, "Real filtered ls differs from source descendants/order")
        def refs_oracle(value: dict[str, Any]) -> None:
            expected = {node["id"] for node in nodes if isinstance(node.get("evidence"), list) and "C05" in node["evidence"]}
            actual = {row["id"] for row in value["structured"] if row["field"] == "evidence"}
            require(actual == expected, "Real refs omitted or invented source claim bindings")
            for row in value["prose"]:
                start, end = row["range"]["start"], row["range"]["end"]
                require((fixture / row["source"]).read_bytes()[start:end].decode() == row["literal"],
                        "Reference byte range does not identify its literal source token")
        def open_oracle(value: dict[str, Any]) -> None:
            child_parents = {parent for parent in parents.values() if parent}
            expected = {node["id"] for node in nodes if node["type"] == "question" and node["id"] not in child_parents}
            actual = {row["id"] for row in value["items"] if "childless_question" in row["reasons"]}
            require(actual == expected, "Real open questions differ from actual source Child relations")
        def path_oracle(value: dict[str, Any]) -> None:
            chain, node = [], "N85"
            while node:
                chain.append(node); node = parents[node]
            require([row["id"] for row in value["steps"]] == list(reversed(chain)), "Real path differs from source nesting")
        real = [(["status", "--json"], status_oracle),
                (["ls", "--type", "dead_end", "--under", "N12", "--json"], list_oracle),
                (["show", "N62", "--with", "parents,children,claims,sessions", "--full", "--json"], show_oracle),
                (["path", "N85", "--json"], path_oracle),
                (["refs", "C05", "--json"], refs_oracle), (["open", "--json"], open_oracle),
                (["find", "tripartite cognitive physical exploration architecture", "--limit", "10", "--json"],
                 lambda value: require("N04" in [row.get("id", row.get("key")) for row in value["results"]], "Pinned topic search missed reviewed source N04"))]
        results = {"pin": PIN, "artifact": artifact_info(fixture), "selectors_confirmed_from_source": ["N12", "N62", "N85", "C05"], "real": [], "generated": []}
        for words, oracle in real:
            self.scenario("reads.real." + words[0], lambda words=words, oracle=oracle: self.timing(fixture, words, 100, oracle))
            results["real"].append(self.evidence["sections"]["reads.real." + words[0]])
        for shape in ("broad", "deep"):
            for count in READ_SIZES:
                root = self.workspace / f"read-{shape}-{count}"
                meta = generate_artifact(root, count, shape)
                cases = [(["status", "--json"], lambda v, n=count: require(v["complete"] and sum(v["counts"].get(k, 0) for k in ("question", "decision", "experiment", "dead_end", "pivot")) == n, "Generated node count mismatch")),
                         (["ls", "--json"], lambda v, n=count: require(sum(row["kind"] in ("question", "decision", "experiment", "dead_end", "pivot") for row in v["entries"]) == n, "Generated ls omitted nodes")),
                         (["show", meta["node_ids"][-1], "--full", "--json"], lambda v, identity=meta["node_ids"][-1]: require(v["entries"][0]["id"] == identity and "uniqueneedle" in v["entries"][0]["title"], "Generated show identity/content mismatch")),
                         (["path", meta["node_ids"][-1], "--json"], lambda v, n=count, s=shape: require(len(v["steps"]) == (n if s == "deep" else 1), "Generated deep path truncated")),
                         (["refs", meta["claim"], "--json"], None), (["open", "--json"], None),
                         (["find", "uniqueneedle", "--limit", "10", "--json"], lambda v, identity=meta["node_ids"][-1]: require(identity in [r.get("id", r.get("key")) for r in v["results"]], "Generated search missed unique matching entry"))]
                for words, oracle in cases:
                    name = f"reads.{shape}.{count}.{words[0]}"
                    self.scenario(name, lambda root=root, words=words, oracle=oracle: self.timing(root, words, 1000, oracle))
                results["generated"].append({"generator": meta, "artifact": artifact_info(root)})
        return results

    def corpus(self) -> dict[str, Any]:
        artifacts = sorted(path.parent.parent for path in self.args.corpus.rglob("trace/exploration_tree.yaml"))
        require(len(artifacts) == 32, f"Expected complete offline 32-artifact paperbench corpus; observed {len(artifacts)} at {self.args.corpus}")
        results = []
        for artifact in artifacts:
            before = artifact_info(artifact)
            try:
                nodes, _ = node_source(artifact)
                node = nodes[0]["id"] if nodes else "N01"
            except (AcceptanceFailure, ValueError, TypeError, KeyError):
                node = "N01"  # Invalid dialect still exercises every command; not a clean parse.
            commands = [["status", "--json"], ["ls", "--json"], ["show", node, "--full", "--json"],
                        ["path", node, "--json"], ["refs", node, "--json"], ["open", "--json"], ["find", "research", "--limit", "10", "--json"]]
            for words in commands:
                name = "corpus." + artifact.relative_to(self.args.corpus).as_posix() + "." + words[0]
                def invoke(artifact=artifact, words=words):
                    value, index = self.command(artifact, words, (0, 1, 2))
                    code = self.evidence["invocations"][index]["exit"]
                    if code == 2:
                        error = value["error"]
                        require(any(token in error["code"].lower() for token in ("io", "read", "discovery", "recovery")), "Corpus exit 2 is not explicit I/O/discovery/recovery")
                    elif code == 1 and words[0] == "status":
                        require(not value.get("complete", True) and value.get("diagnostics", {}).get("errors", 0) > 0, "Invalid corpus status falsely claims completeness")
                    return {"exit": code, "result": value, "invocation": index, "clean_parse_claim": False}
                self.scenario(name, invoke)
            require(artifact_info(artifact)["inventory_sha256"] == before["inventory_sha256"], "Read sweep mutated corpus " + str(artifact))
            results.append(before)
        return {"artifacts": results, "count": len(artifacts), "network": "none", "commands_each": 7}

    def merge(self) -> dict[str, Any]:
        results = []
        for total in MERGE_SIZES:
            case = self.workspace / f"merge-{total}"
            base, ours, theirs, meta = merge_trio(case, total)
            before_base, before_theirs = source_files(base), source_files(theirs)
            original_ours = source_files(ours)
            words = ["merge", "--base", str(base), "--theirs", str(theirs), "--as", "acceptance-fork", "--source-key", "generated_fixture_bob", "--json"]
            def dry_run():
                value, _ = self.command(ours, [*words[:-1], "--dry-run", "--json"])
                require(source_files(ours) == original_ours, "Merge dry run changed source bytes")
                return value
            self.scenario(f"merge.{total}.dry_run", dry_run)
            counter = [0]
            def prepare():
                counter[0] += 1
                target = case / f"sample-{counter[0]}"
                shutil.copytree(ours, target)
                return target
            self.scenario(f"merge.{total}.timing", lambda: self.timing(ours, words, 1000 if total == 10000 else None, prepare=prepare))
            def proof():
                value, _ = self.command(ours, words)
                rows, parents = node_source(ours)
                require(len(rows) == total, "Merge lost or duplicated nodes")
                destination_ids = {r["id"] for r in rows}
                require(all(p is None or p in destination_ids for p in parents.values()), "Merge left unknown nesting parent")
                imported = [r for r in rows if "theirs" in r["title"]]
                require(len(imported) == total // 10, "Merge omitted incoming branch payloads")
                mapping = {item["original"]: item["target"] for item in value["imports"]}
                actual_by_id = {row["id"]: row for row in rows}
                incoming_rows, incoming_parents = node_source(theirs)
                for source_node in incoming_rows:
                    if "theirs" not in source_node["title"]:
                        continue
                    identity = source_node["id"]
                    require(identity in mapping, "Merge omitted complete numeric import mapping " + identity)
                    actual = actual_by_id[mapping[identity]]
                    require(parents[actual["id"]] == mapping.get(incoming_parents[identity], incoming_parents[identity]),
                            "Merge relocated incoming node under the wrong parent")
                    for field, expected in source_node.items():
                        if field == "id":
                            expected = mapping[identity]
                        elif field in ("also_depends_on", "same_as"):
                            expected = [mapping.get(target, target) for target in expected]
                        elif field == "evidence" and isinstance(expected, list):
                            expected = [mapping.get(target, target) for target in expected]
                        elif isinstance(expected, str) and field in ("title", "description", "choice", "result", "hypothesis", "to"):
                            # These synthetic caller strings explicitly name their
                            # own native node. The approved source-side reference
                            # rewrite relocates that token, retaining every other byte.
                            expected = re.sub(r"(?<![\w./:#-])" + re.escape(identity) + r"(?![\w./:#-])", mapping[identity], expected)
                        require(actual.get(field) == expected, "Merge lost exact incoming source field " + identity + "." + field)
                for path in ("logic/claims.md", "logic/concepts.md", "logic/experiments.md", "logic/related_work.md", "logic/solution/heuristics.md", "staging/observations.yaml", "trace/taste_log.yaml", "trace/pm_reasoning_log.yaml"):
                    require(b"theirs" in (ours / path).read_bytes(), "Merge omitted proportional layer " + path)
                require(any("theirs" in (ours / p).read_text() for p in source_files(ours) if p.startswith("trace/sessions/") and p != INDEX), "Merge omitted complete incoming sessions")
                self.command(ours, ["check", str(ours), "--json"], explicit_root=False)
                committed = source_files(ours)
                replay, _ = self.command(ours, words)
                require(source_files(ours) == committed, "Clean merge replay changed source or portable history bytes")
                require(source_files(base) == before_base and source_files(theirs) == before_theirs, "Merge changed read-only input")
                require((ours / "src/untouched.txt").read_bytes() == original_ours["src/untouched.txt"] and (ours / "evidence/untouched.txt").read_bytes() == original_ours["evidence/untouched.txt"], "Merge mutated out-of-scope source/evidence")
                return {"merge": value, "replay": replay, "generator": meta, "phase_times": value.get("timings"), "phase_times_reason": "Observed native CLI clocks; external process distribution includes serialization and byte consumption"}
            self.scenario(f"merge.{total}.preservation_replay", proof)
            results.append({"generator": meta, "base": artifact_info(base), "ours": artifact_info(ours), "theirs": artifact_info(theirs)})
        return {"cases": results, "memory": {"metric": "cumulative child high-water RSS, not per-phase or per-command", "raw": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss, "unit": "bytes on Darwin; KiB on Linux"}}

    def git_command(self, repository: Path, words: list[str]) -> bytes:
        executable = shutil.which("git")
        require(executable is not None, "Offline Git prerequisite is missing")
        argv = [executable, "-c", "core.fsmonitor=false", "-c", "core.hooksPath=" + os.devnull,
                "-c", "core.attributesFile=" + os.devnull, "-c", "commit.gpgSign=false",
                "-c", "protocol.allow=never", "-c", "user.name=Synthetic Acceptance",
                "-c", "user.email=acceptance@example.invalid", *words]
        environment = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
        environment.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull,
                            "GIT_ATTR_NOSYSTEM": "1", "GIT_NO_LAZY_FETCH": "1", "GIT_NO_REPLACE_OBJECTS": "1",
                            "GIT_TERMINAL_PROMPT": "0", "GIT_OPTIONAL_LOCKS": "0",
                            "GIT_AUTHOR_DATE": "2026-10-01T10:00:00+0000",
                            "GIT_COMMITTER_DATE": "2026-10-01T10:00:00+0000"})
        start = time.perf_counter_ns()
        process = subprocess.Popen(argv, cwd=repository, env=environment, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        timed_out = False
        try:
            stdout, stderr = process.communicate(timeout=self.args.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            process.kill(); stdout, stderr = process.communicate()
        record = {"argv": argv, "cwd": str(repository), "exit": process.returncode,
                  "elapsed_ms": (time.perf_counter_ns() - start) / 1e6, "timed_out": timed_out,
                  "stdout": stdout.decode("utf-8", "replace"), "stderr": stderr.decode("utf-8", "replace"),
                  "stdout_bytes": len(stdout), "stderr_bytes": len(stderr)}
        self.evidence.setdefault("git_setup_invocations", []).append(record)
        require(not timed_out and process.returncode == 0, "Git fixture setup failed: " + dump(record))
        return stdout

    def git(self) -> dict[str, Any]:
        results = []
        for total in MERGE_SIZES:
            for repository_shape in ("small", "unrelated-tree"):
                name = f"git.{total}.{repository_shape}"
                def prepare_case(total=total, repository_shape=repository_shape):
                    case = self.workspace / f"git-{total}-{repository_shape}"
                    base, ours, theirs, metadata = merge_trio(case, total)
                    original_ours, original_base, original_theirs = source_files(ours), source_files(base), source_files(theirs)
                    repository = case / "repository"
                    repository.mkdir()
                    shutil.copytree(base, repository / "ara")
                    if repository_shape == "unrelated-tree":
                        for number in range(1000):
                            save(repository, f"unrelated/group-{number // 50:02}/entry-{number:04}.txt",
                                 f"Synthetic unrelated committed tree entry {number}\n")
                    self.git_command(repository, ["init", "--template=", "--initial-branch=ours"])
                    self.git_command(repository, ["add", "--all"])
                    self.git_command(repository, ["commit", "--no-verify", "-m", "Synthetic shared base"])
                    base_commit = self.git_command(repository, ["rev-parse", "HEAD"]).decode().strip()
                    self.git_command(repository, ["checkout", "-b", "theirs"])
                    shutil.rmtree(repository / "ara"); shutil.copytree(theirs, repository / "ara")
                    self.git_command(repository, ["add", "--all"])
                    self.git_command(repository, ["commit", "--no-verify", "-m", "Synthetic incoming fork"])
                    theirs_commit = self.git_command(repository, ["rev-parse", "HEAD"]).decode().strip()
                    self.git_command(repository, ["checkout", "ours"])
                    shutil.rmtree(repository / "ara"); shutil.copytree(ours, repository / "ara")
                    self.git_command(repository, ["add", "--all"])
                    self.git_command(repository, ["commit", "--no-verify", "-m", "Synthetic destination fork"])
                    head_commit = self.git_command(repository, ["rev-parse", "HEAD"]).decode().strip()
                    provenance = {"repo_relative_root": "ara", "base": base_commit, "head": head_commit, "theirs": theirs_commit}
                    words = ["merge", "--git", "theirs", "--source-key", "generated_fixture_bob",
                             "--as", "acceptance-fork", "--no-duplicate-check", "--json"]
                    before_state = git_state(repository)
                    def dry_run():
                        value, index = self.command(repository / "ara", [*words[:-1], "--dry-run", "--json"])
                        require(value["git"] == provenance, "Git dry run consumed unpinned commits or another artifact root")
                        require(source_files(repository / "ara") == original_ours and git_state(repository) == before_state,
                                "Git dry run changed artifact/index/refs/outside worktree")
                        return {"report": value, "invocation": index, "before_git_state": before_state}
                    self.scenario(name + ".dry_run", dry_run)
                    counter = [0]
                    def sample():
                        counter[0] += 1
                        target = case / f"sample-{counter[0]}"
                        shutil.copytree(repository, target)
                        return target / "ara"
                    def oracle(value):
                        require(value["git"] == provenance, "Git timing run changed pinned source identities")
                        require(value.get("unresolved_count") == 0, "Valid generated Git history is not a clean merge")
                    self.scenario(name + ".timing", lambda: self.timing(repository / "ara", words, None, oracle, prepare=sample))
                    def proof():
                        directory = case / "directory-result"
                        shutil.copytree(ours, directory)
                        directory_words = ["merge", "--base", str(base), "--theirs", str(theirs),
                                           "--source-key", "generated_fixture_bob", "--as", "acceptance-fork",
                                           "--no-duplicate-check", "--json"]
                        directory_report, directory_index = self.command(directory, directory_words)
                        report, git_index = self.command(repository / "ara", words)
                        require(report["git"] == provenance, "Git commit provenance differs from observed setup")
                        require(comparable_merge_report(report) == comparable_merge_report(directory_report),
                                "Git and directory full mapping/content/conflict decisions differ")
                        git_files, directory_files = source_files(repository / "ara"), source_files(directory)
                        require(git_files.keys() == directory_files.keys(), "Git candidate omitted or invented artifact paths")
                        differences = []
                        for path in git_files:
                            if path == "trace/merge_log.yaml":
                                left = comparable_merge_journal(yaml_load(repository / "ara" / path))
                                right = comparable_merge_journal(yaml_load(directory / path))
                                require(left == right, "Git and directory portable history differ beyond typed git/time metadata")
                            else:
                                require(git_files[path] == directory_files[path], "Git and directory exact candidate bytes differ: " + path)
                            if git_files[path] != directory_files[path]:
                                differences.append({"path": path, "allowed": "typed merge-journal captured time and Git provenance only"})
                        rows, parents = node_source(repository / "ara")
                        require(len(rows) == total, "Git merge lost complete generated node union")
                        incoming_rows, incoming_parents = node_source(theirs)
                        mapping = {item["original"]: item["target"] for item in report["imports"]}
                        actual = {row["id"]: row for row in rows}
                        for node in incoming_rows:
                            if "theirs" not in node["title"]:
                                continue
                            identity = node["id"]
                            require(identity in mapping, "Git merge omitted incoming source mapping " + identity)
                            imported = actual[mapping[identity]]
                            require(parents[imported["id"]] == mapping.get(incoming_parents[identity], incoming_parents[identity]),
                                    "Git merge relocated source parent")
                            for key, expected in node.items():
                                if key == "id":
                                    expected = mapping[identity]
                                elif key in ("also_depends_on", "same_as") or key == "evidence" and isinstance(expected, list):
                                    expected = [mapping.get(target, target) for target in expected]
                                elif isinstance(expected, str) and key in ("title", "description", "choice", "result", "hypothesis", "to"):
                                    expected = re.sub(r"(?<![\w./:#-])" + re.escape(identity) + r"(?![\w./:#-])", mapping[identity], expected)
                                require(imported.get(key) == expected, "Git merge lost complete source field " + identity + "." + key)
                        require(git_state(repository) == before_state, "Git merge changed index/HEAD/refs/merge state/outside worktree")
                        self.command(repository / "ara", ["check", str(repository / "ara"), "--json"], explicit_root=False)
                        replay, replay_index = self.command(repository / "ara", words)
                        require(source_files(repository / "ara") == git_files and git_state(repository) == before_state,
                                "Git replay changed portable content or repository state")
                        require(replay["git"] == provenance, "Git replay consumed another source revision")
                        require(source_files(ours) == original_ours and source_files(base) == original_base and source_files(theirs) == original_theirs,
                                "Git fixture benchmark altered original directory snapshots")
                        return {"report": report, "directory_report": directory_report, "replay": replay,
                                "invocations": [directory_index, git_index, replay_index], "candidate": artifact_info(repository / "ara"),
                                "allowed_byte_differences": differences, "before_git_state": before_state,
                                "after_git_state": git_state(repository), "complete_source_mapping": True}
                    self.scenario(name + ".equivalence_replay", proof)
                    return {"generator": metadata, "repository_shape": repository_shape, "git": provenance,
                            "artifact": artifact_info(ours), "committed_input_files": len(original_base) + len(original_theirs),
                            "committed_input_body_bytes": sum(map(len, original_base.values())) + sum(map(len, original_theirs.values())),
                            "unrelated_committed_entries": 1000 if repository_shape == "unrelated-tree" else 0}
                self.scenario(name + ".setup", prepare_case)
                results.append(self.evidence["sections"][name + ".setup"])
        return {"cases": results, "latency_threshold": None, "threshold_status": "No approved Git latency threshold; phase and full process distributions are reported",
                "phase_limits": "Tree capture and blob materialization are overlapping cumulative pipeline spans including backpressure; never sum them",
                "storage_limits": "Committed input bytes are source volume, not observed peak temporary storage; native temp paths/child counts are not exposed",
                "memory": {"metric": "cumulative child high-water RSS, not isolated Git command RSS", "raw": resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,
                           "unit": "bytes on Darwin; KiB on Linux"}}

    def replay(self) -> dict[str, Any]:
        fixture = self.args.fixture
        nodes, parents = node_source(fixture)
        by_id = {node["id"]: node for node in nodes}
        order, seen, origins = [], set(), {}
        for path in sorted((fixture / "trace/sessions").glob("*.yaml")):
            if path.name in ("session_index.yaml", "_index.yaml"):
                continue
            document = yaml_load(path)
            for ordinal, event in enumerate(document.get("events_logged", [])):
                identity = event.get("id")
                if identity in by_id and identity not in seen:
                    seen.add(identity); order.append(identity)
                    origins[identity] = {"session": path.name, "turn": event.get("turn"), "ordinal": ordinal}
        missing = [node["id"] for node in nodes if node["id"] not in seen]
        recorded_order = order.copy()
        order, inferred_prerequisites = replay_sequence(nodes, parents, recorded_order)
        operations, incompatible = [], replay_incompatibilities(nodes, parents)
        created = set()
        for identity in order:
            node = by_id[identity]
            parent = parents[identity]
            if parent and parent not in created:
                incompatible.append({"id": identity, "kind": "parent_not_yet_created_in_session_order", "parent": parent, "origin": origins.get(identity)})
            fields = {key: value for key, value in node.items() if key not in ("id", "type", "title", "also_depends_on")}
            for legacy in ("prior_direction", "new_direction", "reason", "parent"):
                if legacy in fields:
                    incompatible.append({"id": identity, "kind": "legacy_or_explicit_source_field", "field": legacy, "value": fields[legacy]})
            operations.append({"op": "node.add", "id": identity, "type": node["type"], "parent": parent or "root", "title": node["title"], "fields": fields, "depends_on": node.get("also_depends_on", [])})
            created.add(identity)
        target = self.workspace / "pinned-node-replay"
        shutil.copytree(fixture, target)
        save(target, TREE, "tree: []\n")
        replay_input = self.workspace / "pinned-node-replay.jsonl"
        replay_input.write_text("\n".join(map(dump, operations)) + "\n")
        result = {"pin": PIN, "source_nodes": len(nodes), "operations": len(operations), "apply_order": order,
                  "recorded_session_event_order": recorded_order, "inferred_prerequisites": inferred_prerequisites,
                  "session_origins": origins, "nodes_without_session_event": missing, "dialect_observations": incompatible,
                  "input": str(replay_input), "input_sha256": sha(replay_input.read_bytes()), "silently_skipped_nodes": 0,
                  "full_historical_chronology_claim": False,
                  "source_tree": {"path": str(fixture / TREE), "sha256": sha((fixture / TREE).read_bytes()),
                                  "bytes": len((fixture / TREE).read_bytes()), "utf8": (fixture / TREE).read_bytes().decode("utf-8")},
                  "engineering_compatibility": "blocked: historical import contract required" if incompatible else "awaiting actual source comparison",
                  "experiment_E3": "deferred; this is deterministic engineering replay, not the paper experiment"}
        self.evidence["replay_input"] = result
        self.apply(target, operations)
        actual, actual_parents = node_source(target)
        require({n["id"]: n for n in actual} == by_id, "Pinned replay lost or changed a complete source node field/dependency")
        require(actual_parents == parents, "Pinned replay changed nesting parents")
        result["complete_node_parent_dependency_source_comparison"] = True
        return result

    def load_inventory(self) -> dict[str, Any]:
        inventory = json.loads(self.args.inventory.read_bytes())
        require(inventory["format"] == "ara.skill-operations/v1", "Wrong inventory schema")
        require(len(inventory["operations"]) >= 104, "Pinned inventory was narrowed below the original 104 required rows")
        lock_path = self.args.inventory.parent / inventory["source_lock"]
        lock = json.loads(lock_path.read_bytes())
        files = {row["source_path"]: row for row in lock["files"]}
        pins = {pin["pin_id"]: pin for pin in lock["pins"]}
        archive = self.args.inventory.parent / next(iter(pins.values()))["archive_root"]
        verified = {}
        for source_path, record in files.items():
            path = archive / source_path
            content = path.read_bytes()
            require(hashlib.sha256(content).hexdigest() == record["sha256"] and len(content) == record["bytes"], "Archived source lock mismatch: " + source_path)
            verified[source_path] = content
        for row in inventory["operations"]:
            identity = row["operation_id"]
            require(identity not in self.rows, "Duplicate inventory row " + identity)
            require(row["skill_pin"] in pins and pins[row["skill_pin"]]["full_revision"] == PIN, "Unverified skill pin")
            clause = row["source_clause"]
            content = verified[clause["path"]]
            selected = "\n".join(content.decode().splitlines()[clause["line_start"] - 1:clause["line_end"]])
            require(clause["quoted_clause"] in selected, "Pinned source clause does not match archive: " + identity)
            self.rows[identity] = {"operation_id": identity, "required": row["required"], "access_policy": row["access_policy"],
                "payload_contract": row["payload_contract"], "history_contract": row["history_contract"],
                "source_proof": {"archive": str(archive / clause["path"]), "revision": PIN, "sha256": sha(content), **clause}, "proofs": []}
        if lock.get("upstream_review_status") != "approved":
            self.evidence["blockers"].append("Pinned protocol/skill contracts are pending upstream review; behavior measurements do not fabricate approval")
        return inventory

    def coverage(self) -> dict[str, Any]:
        inventory = self.load_inventory()
        root = self.workspace / "compiler-fidelity"
        docs = input_documents()
        source_input = self.workspace / "acceptance-input.json"
        source_input.write_text(dump({"count": 1, "documents": docs, "literal": FULL_TEXT}) + "\n")
        def compiler_init():
            self.apply(root, [{"op": "artifact.init", "profile": "compiler", "documents": docs}])
            for path, text in docs.items():
                require((root / path).read_bytes() == text.encode(), "Compiler init changed caller document " + path)
                self.source(root, path)
            require(not (root / "src/environment.md").exists(), "Compiler invented external source content")
            return {"input_sha256": sha(source_input.read_bytes()), "documents_exact": list(docs)}
        self.prove(["compiler.initialize"], "compiler_init", compiler_init)
        def manager_init():
            target = self.workspace / "pm-init"
            self.apply(target, [{"op": "artifact.init", "profile": "research-manager", "paper": docs["PAPER.md"]}])
            require((target / "PAPER.md").read_bytes() == docs["PAPER.md"].encode(), "Manager initializer changed caller PAPER")
            for path, key in [(TREE, "tree"), (INDEX, "sessions"), ("staging/observations.yaml", "observations"), ("trace/pm_reasoning_log.yaml", "entries")]:
                value = yaml_load(target / path)
                require(isinstance(value, dict) and value[key] == [], "Manager initializer did not retain source schema " + path)
            self.apply(target, [{"op": "node.add", "id": "N01", "type": "question", "parent": "root",
                                 "title": "Existing caller research question", "fields": node_fields("question", FULL_TEXT),
                                 "depends_on": []}])
            existing = source_files(target)
            for path in ("staging/observations.yaml", "trace/pm_reasoning_log.yaml"):
                (target / path).unlink()
            self.apply(target, [{"op": "artifact.init", "profile": "research-manager",
                                 "paper": docs["PAPER.md"], "missing_only": True}])
            require(source_files(target) == existing,
                    "Missing-only bootstrap rewrote existing knowledge or failed to restore exact native scaffolds")
            shown, _ = self.command(target, ["show", "N01", "--full", "--json"])
            require(shown["entries"][0]["source_fields"]["description"] == FULL_TEXT,
                    "Missing-only bootstrap erased existing caller node content")
            return artifact_info(target)
        self.prove(["pm.initialize"], "manager_init", manager_init)
        def nodes_proof():
            ops = []
            kinds = ("question", "decision", "experiment", "dead_end", "pivot")
            concepts_before = self.source(root, "logic/concepts.md")
            for number, kind in enumerate(kinds, 1):
                ops.append({"op": "node.add", "id": f"N{number:02}", "type": kind,
                    "parent": "root" if number < 3 else "N01", "title": "Synthetic " + kind, "fields": node_fields(kind, FULL_TEXT), "depends_on": []})
            ops.append({"op": "edge.add", "node": "N03", "depends_on": "N02"})
            ops += [{"op": "node.add", "id": "N07", "type": "question", "parent": "root",
                     "title": "Synthetic existing concept reference",
                     "fields": node_fields("question", FULL_TEXT) | {"timestamp": "2026-10-01T10:05", "concepts": ["Synthetic term"]}},
                    {"op": "node.add", "id": "N06", "type": "question", "parent": "root",
                     "title": "Synthetic later recurrence", "fields": node_fields("question", FULL_TEXT) | {"timestamp": "2026-10-01T10:06", "concepts": ["Synthetic introduced term"]}},
                    {"op": "document.replace", "document": "logic/concepts.md", "expected": sha(concepts_before.encode()),
                     "content": concepts_before + "\n## Synthetic introduced term\n\n- **Definition**: Caller-authored native concept.\n"},
                    {"op": "node.link_same_as", "node": "N06", "same_as": "N07"}]
            before = source_files(root)
            self.apply(root, ops, dry=True)
            require(source_files(root) == before, "Batch dry run changed sources")
            self.apply(root, ops)
            rows, parents = node_source(root); by_id = {r["id"]: r for r in rows}
            for op in (op for op in ops if op["op"] == "node.add"):
                actual = by_id[op["id"]]
                for key, value in {"id": op["id"], "type": op["type"], "title": op["title"], **op["fields"]}.items():
                    require(actual.get(key) == value, "Node payload lost field " + key)
                require(parents[op["id"]] == (None if op["parent"] == "root" else op["parent"]), "Node parent mismatch")
            require(by_id["N03"]["also_depends_on"] == ["N02"], "Cross edge did not persist")
            require(by_id["N06"]["same_as"] == ["N07"], "Later recurrence lost its exact earlier identity")
            require("## Synthetic introduced term\n" in self.source(root, "logic/concepts.md"), "New concept was not grounded in a native heading")
            for invalid in ({"op": "node.link_same_as", "node": "N07", "same_as": "N06"},
                            {"op": "node.link_same_as", "node": "N06", "same_as": "N06"},
                            {"op": "node.link_same_as", "node": "N06", "same_as": "N999"},
                            {"op": "node.add", "type": "question", "parent": "N04", "title": "Invalid child", "fields": node_fields("question", FULL_TEXT)},
                            {"op": "node.add", "type": "question", "parent": "root", "title": "Invalid concept", "fields": node_fields("question", FULL_TEXT) | {"concepts": ["Uncreated synthetic term"]}}):
                before_rejection = source_files(root)
                self.apply(root, [invalid], expected=(1,))
                require(source_files(root) == before_rejection, "Invalid same-as, leaf, or concept request changed source")
            self.command(root, ["show", "N01", "N02", "N03", "N04", "N05", "--with", "parents,children,depends_on", "--full", "--json"])
            rejected_before = source_files(root)
            self.apply(root, [{"op": "node.add", "type": "question", "parent": "root", "title": "Must rollback", "fields": {"description": "would be lost"}}, {"op": "unknown.required-operation"}], expected=(1,))
            require(source_files(root) == rejected_before, "Rejected batch changed source/existence state")
            return {"nodes": by_id, "parents": parents, "rejection_preserved_sources": True}
        self.prove([f"{skill}.add_{kind}" for skill in ("pm", "compiler") for kind in ("question", "decision", "experiment", "dead_end", "pivot")] + ["pm.append_cross_edge", "compiler.append_cross_edge"], "node_payloads_batch", nodes_proof)
        for operation, path in [("write_problem", "logic/problem.md"), ("write_claims", "logic/claims.md"), ("write_concepts", "logic/concepts.md"), ("write_experiments", "logic/experiments.md"), ("write_constraints", "logic/solution/constraints.md"), ("write_heuristics", "logic/solution/heuristics.md"), ("write_related_work", "logic/related_work.md")]:
            def replace(path=path):
                original = self.source(root, path)
                replacement = original + "\n<!-- Synthetic source-grounded coverage repair; exact α -->\n"
                self.apply(root, [{"op": "document.replace", "document": path, "expected": sha(original.encode()), "content": replacement}])
                require(self.source(root, path) == replacement, "Full compiler replacement lost content")
                if path == "logic/solution/heuristics.md":
                    fields = compiler_heuristic_fields()
                    self.apply(root, [{"op": "heuristic.add", "id": "H10", "title": "Synthetic native compiler profile", "fields": fields}])
                    shown, _ = self.command(root, ["show", "H10", "--full", "--json"])
                    actual = {field["name"]: field["value"] for field in shown["entries"][0]["source_fields"]}
                    for key, value in fields.items():
                        require(actual.get(key) == value, "Compiler heuristic lost exact native " + key)
                    require("Status" not in actual and "Provenance" not in actual and "Sources" not in actual,
                            "Compiler heuristic invented manager-profile values")
                return {"document": path, "before": original, "after": replacement}
            self.prove(["compiler." + operation], operation, replace)
        for operation, path in [("write_architecture", "logic/solution/architecture.md"), ("write_algorithm", "logic/solution/algorithm.md"), ("arbitrary_solution", "logic/solution/domain-specific.md"), ("additional_knowledge", "appendix/domain-notes.md")]:
            def create(path=path):
                if path.startswith("appendix/"):
                    self.apply(root, [{"op": "paper.edit", "frontmatter": {"knowledge_paths": [path]}}])
                text = "# Synthetic " + path + "\n\n## Caller section\n\n" + FULL_TEXT
                self.apply(root, [{"op": "document.create", "document": path, "content": text}])
                require(self.source(root, path) == text, "Created native document not exact")
                return {"document": path, "content": text}
            self.prove(["compiler." + operation], operation, create)
        def rubric():
            # The rubric is a plain file the compiler writes with its own file
            # tool; ara rejects native rubric creation and reads.
            path = "rubric/requirements.md"
            text = "# Synthetic supplied rubric\n\n" + markdown_entry("R01", "Caller requirement", {
                "Rubric ID": "00000000-0000-4000-8000-000000000001", "Category": "synthetic",
                "Weight": "1", "Requirement": FULL_TEXT, "ARA coverage": "logic/claims.md#C01",
                "Key detail": "Exact source content, not generated paraphrase"})
            before = source_files(root)
            rejected = self.apply(root, [{"op": "document.create", "document": path, "content": text}], expected=(1,))
            require(rejected["error"]["code"] == "write.document", "Native rubric creation was not rejected")
            require(source_files(root) == before, "Rejected native rubric creation changed source bytes")
            (root / "rubric").mkdir(exist_ok=True)
            (root / path).write_bytes(text.encode())
            read, _ = self.command(root, ["show", "--document", path, "--json"], expected=(1,))
            require(read["error"]["code"] == "invalid_document" and "rubric/" in read["error"]["details"]["file_access"],
                    "Native rubric read did not point to direct file access")
            require((root / path).read_bytes() == text.encode(), "Direct rubric file not exact")
            return {"document": path, "content": text, "native_create_rejected": True}
        self.prove(["compiler.rubric"], "rubric", rubric)
        def paper():
            body = self.source(root, "PAPER.md", ("Layer Index",))
            content = "\nCaller-selected revised layer index — α.\n"
            self.apply(root, [{"op": "paper.edit", "frontmatter": {"title": "Synthetic caller-revised title", "authors": ["Caller"], "venue": "Synthetic", "year": 2026, "doi": "pending", "domain": "synthetic", "keywords": ["retention"], "claims_summary": ["C01"], "abstract": FULL_TEXT}, "heading": ["Layer Index"], "expected": sha(body.encode()), "content": content}])
            full = self.source(root, "PAPER.md")
            require("extension: {opaque: [α, null, 3]}" in full and content in full, "PAPER lost unknown metadata or complete index")
            return {"source": full}
        self.prove(["compiler.write_paper"], "paper_unknown_metadata", paper)
        def full_turn():
            ops = [{"op": "session.start", "id": SESSION, "date": "2026-10-01", "started": "2026-10-01T10:00", "summary": "Synthetic initial session"},
                   {"op": "observation.stage", "id": "O01", "content": FULL_TEXT, "potential_type": "claim", "context": "Caller selected exact context", "provenance": "ai-suggested", "timestamp": "2026-10-01T10:01", "bound_to": ["N03"]},
                   {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:02", "summary": "Synthetic complete rolling summary",
                    "events": [{"type": "observation", "id": "O01", "routing": "staged", "provenance": "ai-suggested", "summary": FULL_TEXT}],
                    "ai_actions": [{"action": FULL_TEXT, "provenance": "ai-executed", "files_changed": ["logic/claims.md"]}],
                    "claims_touched": [{"id": "C01", "action": "revised"}],
                    "logic_revisions": [{"entry": "C01", "field": "Evidence basis", "before": "Historical caller value\n", "after": FULL_TEXT, "signal": "user-directive", "provenance": "user", "note": "Historical import, not claimed coupled edit"}],
                    "key_context": [{"excerpt": FULL_TEXT}], "open_threads": ["Caller open thread\nα"], "ai_suggestions_pending": ["Unconfirmed caller suggestion"]},
                   {"op": "record.append", "document": "trace/pm_reasoning_log.yaml", "record": {"turn": SESSION + "#1", "notes": [FULL_TEXT, "Rejected near-miss: maybe is not affirmation"]}}]
            self.apply(root, ops)
            observation = yaml_load(root / "staging/observations.yaml")["observations"][0]
            for key in ("content", "context", "potential_type", "provenance", "timestamp", "bound_to"):
                require(observation[key] == ops[1][key], "Stage lost field " + key)
            require(observation["promoted"] is False and observation["promoted_to"] is None and observation["stale"] is False, "Stage initial pointers wrong")
            session = yaml_load(root / f"trace/sessions/{SESSION}.yaml")
            for payload_key, source_key in [("events", "events_logged"), ("ai_actions", "ai_actions"), ("claims_touched", "claims_touched"), ("logic_revisions", "logic_revisions"), ("key_context", "key_context")]:
                require(session[source_key] == [record | {"turn": 1} for record in ops[2][payload_key]], "Session lost full array " + source_key)
            for key in ("open_threads", "ai_suggestions_pending"):
                require(session[key] == ops[2][key], "Session lost rolling list " + key)
            require(session["session"]["turn_count"] == 1 and session["session"]["summary"] == ops[2]["summary"] and session["session"]["started"] == ops[0]["started"], "Session metadata incoherent")
            index = yaml_load(root / INDEX)["sessions"]
            row = next(row for row in index if row["id"] == SESSION)
            require(row["turn_count"] == 1 and row["events_count"] == 1 and row["claims_touched"] == ["C01"] and row["open_threads"] == 1, "Index not derived from full record")
            reasoning = yaml_load(root / "trace/pm_reasoning_log.yaml")["entries"]
            require(ops[3]["record"] in reasoning, "Reasoning lost rejected near-miss/exact notes")
            self.source(root, f"trace/sessions/{SESSION}.yaml"); self.source(root, INDEX); self.source(root, "staging/observations.yaml"); self.source(root, "trace/pm_reasoning_log.yaml")
            return {"session": session, "observation": observation, "index": row, "reasoning": reasoning}
        self.prove(["pm.stage_observation", "pm.create_session", "pm.update_session_metadata", "pm.update_session_index", "pm.append_reasoning"] + ["pm.append_session_" + key for key in ("events_logged", "ai_actions", "claims_touched", "logic_revisions", "key_context")] + ["pm.update_session_open_threads", "pm.update_session_ai_suggestions_pending"], "full_manager_turn", full_turn)
        for kind in ("claim", "heuristic", "concept", "constraint", "architecture", "dead_end"):
            def promote(kind=kind):
                number = ("claim", "heuristic", "concept", "constraint", "architecture", "dead_end").index(kind) + 2
                oid = f"O{number:02}"
                fields = claim_fields() if kind == "claim" else heuristic_fields() if kind == "heuristic" else {"Definition": FULL_TEXT} if kind == "concept" else node_fields("dead_end", FULL_TEXT) if kind == "dead_end" else {"Description": FULL_TEXT}
                signal = "empirical-resolution" if kind == "dead_end" else "verbal-affirmation"
                promotion = {"op": "observation.promote", "observation": oid, "to": kind, "title": "Synthetic promoted " + kind, "fields": fields, "signal": signal}
                self.apply(root, [{"op": "observation.stage", "id": oid, "content": FULL_TEXT, "context": "Synthetic promotion signal supplied by caller", "potential_type": "unknown" if kind == "dead_end" else kind, "provenance": "ai-suggested", "timestamp": "2026-10-01T10:03", "bound_to": ["N03"]}, promotion,
                    {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:04", "key_context": [{"excerpt": "Synthetic caller signal; not inferred from silence"}]}])
                obs = next(o for o in yaml_load(root / "staging/observations.yaml")["observations"] if o["id"] == oid)
                require(obs["content"] == FULL_TEXT and obs["promoted"] is True and obs["crystallized_via"] == signal and obs["provenance"] == "ai-suggested", "Promotion lost source or upgraded provenance")
                target = obs["promoted_to"]
                # The writer derives the turn's events: one stage and one crystallization.
                session = yaml_load(root / f"trace/sessions/{SESSION}.yaml")
                turn = session["session"]["turn_count"]
                events = [row for row in session["events_logged"] if row["turn"] == turn]
                document, _, section = target.partition("#")
                crystallized = {"type": kind, "routing": "crystallized", "provenance": "ai-suggested", "summary": promotion["title"], "turn": turn}
                crystallized |= {"id": oid, "target": {"document": document, "heading": [section]}} if section else {"id": target.rsplit(":", 1)[1]}
                require(events == [{"type": "observation", "id": oid, "routing": "staged", "provenance": "ai-suggested", "summary": FULL_TEXT, "turn": turn}, crystallized], "Promotion turn lacks its derived stage and crystallization events")
                if target.startswith("trace:"):
                    rows, _ = node_source(root); created = next(n for n in rows if n["id"] == target.split(":", 1)[1])
                    require(created["type"] == "dead_end" and created["hypothesis"] == FULL_TEXT, "Refutation did not retain full dead end")
                else:
                    document = target.split("#", 1)[0].split(":", 1)[0]
                    require(FULL_TEXT in self.source(root, document).replace("\n  ", "\n"), "Promotion target dropped multiline caller value")
                    if kind == "heuristic":
                        shown, _ = self.command(root, ["show", target.split(":", 1)[1], "--full", "--json"])
                        actual = {field["name"]: field["value"] for field in shown["entries"][0]["source_fields"]}
                        require(all(actual.get(key) == (value if isinstance(value, str) else dump(value))
                                    for key, value in fields.items()), "Manager heuristic lost caller profile or list values")
                before = source_files(root)
                self.apply(root, [promotion], expected=(1,))
                require(source_files(root) == before, "Duplicate promotion changed artifact")
                return obs
            self.prove(["pm.promote_" + kind], "promote_" + kind, promote)
        self.manager_mutations(root)
        self.read_coverage(root)
        for row in inventory["operations"]:
            identity = row["operation_id"]
            if row["access_policy"] != "cli-required":
                # Exemptions are explicit source-contract policy, never invented to pass.
                self.rows[identity]["status"] = "exempt-" + row["access_policy"]
            else:
                proofs = self.rows[identity]["proofs"]
                self.rows[identity]["status"] = "passed" if proofs and all(p["passed"] for p in proofs) else "failed" if proofs else "unexercised"
                if self.rows[identity]["status"] != "passed":
                    self.evidence["failures"].append({"scenario": "coverage", "operation_id": identity, "message": "Required pinned operation lacks passing consumer-visible content/history proof"})
        self.evidence["operation_coverage"] = list(self.rows.values())
        return {"inventory": str(self.args.inventory), "inventory_sha256": sha(self.args.inventory.read_bytes()), "rows": len(self.rows), "source_archive_verified": True,
                "passed": sum(r["status"] == "passed" for r in self.rows.values()), "input": str(source_input), "input_sha256": sha(source_input.read_bytes())}

    def manager_mutations(self, root: Path) -> None:
        def revise():
            previous = yaml_load(root / f"trace/sessions/{SESSION}.yaml")
            turn = previous["session"]["turn_count"] + 1
            before_claim, before_heuristic = self.source(root, "logic/claims.md"), self.source(root, "logic/solution/heuristics.md")
            before_term = "Exact fixture definition."
            sets = [( {"id": "C01"}, {"Statement": "Revised " + FULL_TEXT, "Status": "testing", "Dependencies": ["C02"]}),
                    ({"id": "H01"}, {"Rationale": "Revised " + FULL_TEXT}),
                    ({"document": "logic/concepts.md", "heading": ["Synthetic term"]}, {"Definition": "Revised " + FULL_TEXT})]
            ops = [{"op": "logic.revise", "target": target, "set": fields, "session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "user-revised", "note": "Caller explicitly narrowed the synthetic scope"} for target, fields in sets]
            ops += [{"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:05", "claims_touched": [{"id": "C01", "action": "revised"}], "open_threads": [], "ai_suggestions_pending": []}]
            self.apply(root, ops)
            session = yaml_load(root / f"trace/sessions/{SESSION}.yaml")
            records = [r for r in session["logic_revisions"] if r["turn"] == turn]
            expected = [("Statement", FULL_TEXT), ("Rationale", FULL_TEXT), ("Definition", before_term), ("Status", "hypothesis")]
            for field, before in expected:
                require(any(r["field"] == field and r["before"] == before and r["after"] == ("testing" if field == "Status" else "Revised " + FULL_TEXT) for r in records), "Coupled revision lost exact before/after " + field)
            require(any(r["field"] == "Dependencies" and r["before"] == "[]" and r["after"] == '["C02"]'
                        for r in records), "Dependency repair lost exact before/after list values")
            claim, _ = self.command(root, ["show", "C01", "--full", "--json"])
            require(claim["entries"][0].get("deps") == ["C02"], "Dependency repair did not affect actual query edges")
            for key in ("events_logged", "ai_actions", "claims_touched", "logic_revisions", "key_context"):
                require(session[key][:len(previous[key])] == previous[key], "Revision changed earlier session records")
            require(session["open_threads"] == [] and session["ai_suggestions_pending"] == [], "Explicit empty rolling lists did not clear")
            require("Last revised" in self.source(root, "logic/claims.md"), "Revision pointer missing")
            return {"before_claim": before_claim, "before_heuristic": before_heuristic, "exact_revisions": records}
        self.prove(["pm.revise_claim", "pm.revise_heuristic", "pm.revise_concept", "pm.status_transition", "pm.repair_dependencies"], "coupled_revisions", revise)
        def structural_workflows():
            previous = yaml_load(root / f"trace/sessions/{SESSION}.yaml"); turn = previous["session"]["turn_count"] + 1
            ops = [{"op": "logic.revise", "target": {"id": "C01"}, "set": {"Statement": "Synthetic narrowed primary scope"}, "session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "user"},
                   {"op": "claim.add", "id": "C20", "title": "Synthetic spin-off", "fields": claim_fields("Synthetic spin-off scope")},
                   {"op": "claim.add", "id": "C21", "title": "Synthetic generalized relationship", "fields": claim_fields("Synthetic generalized scope") | {"Dependencies": ["C01", "C20"], "Proof": ["pending"]}},
                   {"op": "logic.revise", "target": {"id": "C20"}, "set": {"Status": "withdrawn", "Merged into": "C01"}, "session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "user"},
                   {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:06", "logic_revisions": [{"entry": "C01", "field": "split", "before": "Revised " + FULL_TEXT, "after": "C01 = primary; C20 = spin-off", "signal": "user-directive", "provenance": "user"}, {"entry": "C20", "field": "merge", "before": "Synthetic spin-off scope", "after": "withdrawn; Merged into: C01", "signal": "user-directive", "provenance": "user"}], "claims_touched": [{"id": "C01", "action": "split"}, {"id": "C20", "action": "merged"}, {"id": "C21", "action": "created"}]}]
            self.apply(root, ops)
            before_rejection = source_files(root)
            content = self.source(root, "logic/claims.md")
            start = content.index("## C20:"); stop = content.find("\n## ", start + 1)
            selected = content[start:] if stop < 0 else content[start:stop + 1]
            rejected = self.apply(root, [{"op": "entry.remove", "target": {"id": "C20"}, "expected": sha(selected.encode()),
                                         "session": SESSION, "turn": turn + 1, "signal": "user-directive", "provenance": "user"}],
                                  expected=(1,))
            require(rejected["error"]["code"] == "write.claim_retention", "Claim deletion did not fail its retention contract")
            require(source_files(root) == before_rejection, "Physical claim deletion changed a retained claim")
            content = self.source(root, "logic/claims.md")
            require(all("## " + identity + ":" in content for identity in ("C01", "C20", "C21")), "Split/generalize/merge deleted a required grounding entry")
            value, _ = self.command(root, ["show", "C21", "C20", "--full", "--json"])
            require(value["entries"][0].get("deps") == ["C01", "C20"] and value["entries"][1].get("status") == "withdrawn", "Generalized dependencies or withdrawn endpoint missing")
            # The exact Merged into pointer is required current-state data, not only history.
            require("Merged into" in content, "Required merge redirect is missing from current-state higher claim")
            return {"claims": content}
        self.prove(["pm.split", "pm.merge", "pm.generalize"], "split_merge_generalize", structural_workflows)
        for kind in ("rename", "remove"):
            def structural(kind=kind):
                name = "Synthetic term" if kind == "rename" else "Synthetic renamed term"
                target = {"document": "logic/concepts.md", "heading": [name]}
                content = self.source(root, "logic/concepts.md")
                start = content.index("## " + name + "\n"); stop = content.find("\n## ", start + 1)
                selected = content[start:] if stop < 0 else content[start:stop + 1]
                turn = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["session"]["turn_count"] + 1
                operation = {"op": "entry." + kind, "target": target, "expected": sha(selected.encode()), "session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "user"}
                if kind == "rename":
                    operation["name"] = "Synthetic renamed term"
                else:
                    operation["redirect"] = {"document": "logic/concepts.md", "heading": ["Synthetic promoted concept"]}
                self.apply(root, [operation, {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:07"}])
                after = self.source(root, "logic/concepts.md")
                require(("## Synthetic renamed term\n" in after) is (kind == "rename"), "Structural identity operation did not affect source entry")
                mutations = yaml_load(root / "trace/logic_mutations.yaml")["mutations"]
                require(any(row["action"] == kind and row["session"] == SESSION for row in mutations), "Native redirect/history not persisted")
                session = yaml_load(root / f"trace/sessions/{SESSION}.yaml")
                require(any(r["turn"] == turn and r["field"] in ("entry", "id", "remove", "rename") and isinstance(r["before"], str) for r in session["logic_revisions"]), "Structural complete endpoint history missing")
                return {"source": after, "mutations": mutations}
            self.prove(["pm.rename_identity" if kind == "rename" else "pm.remove_current_entry"], "identity_" + kind, structural)
        for kind, path, heading in (("constraint", "logic/solution/constraints.md", "Source boundary"),
                                    ("architecture", "logic/solution/architecture.md", "Caller section")):
            def body(path=path, heading=heading, kind=kind):
                before = self.source(root, path, (heading,))
                after = before + "\nCaller complete source body — α.\n\n" + FULL_TEXT
                if kind == "architecture":
                    after += "\n```mermaid\nflowchart LR\n Input --> Knowledge --> Output\n```\n"
                turn = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["session"]["turn_count"] + 1
                self.apply(root, [{"op": "logic.revise", "target": {"document": path, "heading": [heading]},
                    "set": {"Body": after}, "expected": sha(before.encode()), "session": SESSION, "turn": turn,
                    "signal": "user-directive", "provenance": "user"},
                    {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:07"}])
                require(after in self.source(root, path), "Manager selected body lost caller content")
                actual_after = self.source(root, path, (heading,))
                history = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["logic_revisions"]
                require(any(row["turn"] == turn and row["field"] == "Body" and row["before"] == before
                            and row["after"] == actual_after for row in history), "Whole-body revision lost exact final-source endpoint history")
                return {"document": path, "heading": heading, "before": before, "caller_after": after, "actual_after": actual_after}
            self.prove(["pm.write_" + kind + "_body"] + (["compiler.mirror_diagram"] if kind == "architecture" else []), "body_" + kind, body)
        def taste():
            record = {"date": "2026-10-01", "tag": "uncertain", "object": "framing", "comment": FULL_TEXT}
            self.apply(root, [{"op": "entry.taste_append", "target": {"id": "C01"}, "record": record}, {"op": "entry.taste_append", "target": {"id": "H01"}, "record": record}, {"op": "record.append", "id": "T01", "document": "trace/taste_log.yaml", "record": {"timestamp": "2026-10-01T10:08", "target": "N02", "tag": "uncertain", "object": "framing", "comment": FULL_TEXT}}])
            for path in ("logic/claims.md", "logic/solution/heuristics.md"):
                source = self.source(root, path)
                lines = record["comment"].split("\n")
                header = f"  - [{record['date']}] `{record['tag']}` on `{record['object']}` — "
                at = source.index(header)
                stored = source[at + len(header):].split("\n")
                recovered = [stored[0]]
                for line in stored[1:]:
                    if not line.startswith("    "):
                        break
                    recovered.append(line[4:])
                require("\n".join(recovered) == record["comment"], "Native inline taste lost complete caller date/tag/object/comment")
            entries = yaml_load(root / "trace/taste_log.yaml")["entries"]
            require(entries[0]["id"] == "T01" and entries[0]["comment"] == FULL_TEXT and entries[0]["target"] == "N02", "Trace taste payload/pointer missing")
            return {"inline": record, "trace": entries}
        self.prove(["pm.append_logic_taste", "pm.append_trace_taste"], "taste", taste)
        def conflicts_reports():
            before_nodes, _ = node_source(root)
            self.apply(root, [{"op": "entry.annotate", "target": {"id": "C01"}, "kind": "conflict", "references": ["N02"], "comment": "Synthetic unverifiable reader report; repro: compare exact source"}, {"op": "entry.annotate", "target": {"id": "N02"}, "kind": "conflict", "references": ["C01"], "comment": "Retain both supplied bodies"}, {"op": "node.add", "id": "N30", "type": "decision", "parent": "root", "title": "Synthetic unresolved contradiction", "fields": node_fields("decision", "Unverifiable: no corroborating evidence; preserve repro") | {"status": "unresolved"}}])
            content = self.source(root, "logic/claims.md")
            require("CONFLICT" in content, "Conflict not represented using native source annotation")
            rows, _ = node_source(root); decision = next(n for n in rows if n["id"] == "N30")
            require(decision["status"] == "unresolved", "Unverifiable report incorrectly adjudicated resolved")
            turn = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["session"]["turn_count"] + 1
            self.apply(root, [{"op": "logic.revise", "target": {"id": "C01"}, "set": {"Conditions": "Synthetic upheld report narrows boundary"}, "session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "ai-suggested", "note": "Synthetic upheld report basis: exact caller input"}, {"op": "node.add", "id": "N31", "type": "decision", "parent": "root", "title": "Synthetic upheld report", "fields": node_fields("decision", "Upheld by exact caller evidence; C01 corrected") | {"status": "resolved"}}, {"op": "node.add", "id": "N32", "type": "decision", "parent": "root", "title": "Synthetic rejected report", "fields": node_fields("decision", "Rejected: exact caller input contradicts report; target unchanged") | {"status": "resolved"}}, {"op": "observation.stage", "id": "O30", "content": "Synthetic uncrystallized report", "context": "No logic target; ordinary staging", "potential_type": "unknown", "provenance": "ai-suggested", "timestamp": "2026-10-01T10:09"}, {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:10", "key_context": [{"excerpt": "Synthetic report verdicts: upheld/rejected/unverifiable/staged; caller owns judgments"}]}])
            require("Synthetic upheld report narrows boundary" in self.source(root, "logic/claims.md"), "Upheld report failed content correction")
            after_nodes = {node["id"]: node for node in node_source(root)[0]}
            require(all(all(after_nodes.get(old["id"], {}).get(key) == value for key, value in old.items())
                        for old in before_nodes), "Conflict/adjudication changed protected prior node fields")
            return {"unresolved": decision, "caller_adjudication_only": True}
        self.prove(["pm.annotate_contradiction", "pm.adjudicate_reader_report"], "contradictions_reader_reports", conflicts_reports)
        def stale():
            self.apply(root, [{"op": "observation.stage", "id": "O40", "content": FULL_TEXT, "potential_type": "unknown", "provenance": "ai-suggested", "timestamp": "2026-09-27T10:00"}])
            ops, dates = [], ["2026-09-28", "2026-09-29", "2026-09-30"]
            for date in dates:
                sid = date + "_001"
                ops += [{"op": "session.start", "id": sid, "date": date, "started": date + "T10:00", "summary": "Synthetic unrelated day"}, {"op": "session.log", "session": sid, "timestamp": date + "T10:01", "key_context": [{"excerpt": "Synthetic unrelated activity"}]}]
            reason = "Caller triage: retain this idle observation for review; do not discard or crystallize.\n  Exact why = α\n"
            owner = dates[-1] + "_001"
            audit = {"session": owner, "turn": 1, "signal": "user-directive", "provenance": "user",
                     "note": "Caller selected the final newly logged synthetic turn."}
            ops += [{"op": "observation.mark_stale", "observation": "O40", "session_days": dates,
                     "reason": reason, "audit": audit}]
            original = next(o for o in yaml_load(root / "staging/observations.yaml")["observations"] if o["id"] == "O40")
            self.apply(root, ops)
            obs = next(o for o in yaml_load(root / "staging/observations.yaml")["observations"] if o["id"] == "O40")
            require(obs == original | {"stale": True}, "Stale handling changed prose, pointers, provenance, or auto-promoted")
            notes = next(row["notes"] for row in yaml_load(root / "trace/pm_reasoning_log.yaml")["entries"]
                         if row["turn"] == owner + "#1" and row.get("notes", [None])[0] == reason)
            require(len(notes) == 3 and notes[0] == reason and notes[2] == audit["note"], "Stale reasoning lost exact caller why/note")
            evidence = json.loads(notes[1])
            require(evidence == {"operation": "observation.mark_stale", "observation": "O40", "session_days": dates,
                "last_reference": "2026-09-27", "bound_to": [], "signal": audit["signal"], "provenance": audit["provenance"],
                "session_sources": [{"date": date, "document": f"trace/sessions/{date}_001.yaml"} for date in dates],
                "audit": {"session": owner, "turn": 1, "source_refs": [f"trace/sessions/{owner}.yaml"]}},
                "Stale reasoning lacks proven days, exact last reference, or native owned turn")
            require(yaml_load(root / f"trace/sessions/{owner}.yaml")["session"]["turn_count"] == 1,
                    "Stale audit does not belong to the actual newly logged turn")
            self.apply(root, [{"op": "observation.stage", "id": "O41", "content": FULL_TEXT,
                              "potential_type": "unknown", "provenance": "ai-suggested", "timestamp": "2026-09-27T10:00"}])
            before_rejection = source_files(root)
            rejected = self.apply(root, [{"op": "observation.mark_stale", "observation": "O41", "session_days": dates,
                                         "reason": reason, "audit": audit}], expected=(1,))
            require(rejected["error"]["code"] == "write.observation" and rejected["error"].get("details", {}).get("field") == "audit",
                    "Historic owner was not rejected by the stale audit ownership contract")
            require(source_files(root) == before_rejection, "Historic-owner stale rejection changed observation or reasoning")
            return {"observation": obs, "reasoning": notes, "historic_owner_rejected_atomically": True}
        self.prove(["pm.mark_stale"], "three_distinct_session_days", stale)

    def read_coverage(self, root: Path) -> None:
        def full_reads():
            documents = [p for p in source_files(root) if p == "PAPER.md" or p.startswith(("logic/", "trace/", "staging/", "appendix/"))]
            exact = {path: self.source(root, path) for path in documents}
            status, _ = self.command(root, ["status", "--json"])
            listed, _ = self.command(root, ["ls", "--full", "--json"])
            rows, _ = node_source(root)
            require({r["id"] for r in rows}.issubset({r.get("id", r.get("key")) for r in listed["entries"]}), "Native enumeration omitted trace identities")
            require(status["complete"] is True, "Synthetic complete state read refused")
            search, _ = self.command(root, ["find", "Synthetic", "--limit", "1000", "--json"])
            require(search["results"], "Native full-text search omitted matching synthetic content")
            self.command(root, ["ls", "--type", "dead_end", "--full", "--json"])
            self.command(root, ["show", "N02", "N03", "N04", "C01", "H01", "O01", SESSION, "--with", "parents,children,depends_on", "--full", "--json"])
            self.command(root, ["open", "--full", "--json"])
            self.command(root, ["validate", str(root), "--json"], explicit_root=False)
            return {"exact_documents": {path: sha(text.encode()) for path, text in exact.items()}, "status": status, "listed_identities": [r.get("id", r.get("key")) for r in listed["entries"]]}
        def grounding():
            shown, invocation = self.command(root, ["show", "C01", "--full", "--json"])
            fields = {field["name"]: field["value"] for field in shown["entries"][0]["source_fields"]}
            sources = json.loads(fields["Sources"])
            require(isinstance(sources, list) and any('«"count":1»' in source for source in sources), "Native Sources lost its exact quoted input")
            source = (self.workspace / "acceptance-input.json").read_bytes()
            require(b'"count":1' in source and json.loads(source)["count"] == 1, "Native quote does not match actual caller input bytes")
            self.source(root, "logic/claims.md")
            return {"invocation": invocation, "native_sources": fields["Sources"], "input_sha256": sha(source),
                    "quote_verified_against_actual_input": True, "scope": "Access and exact number-source retention, not an epistemic judgment"}
        self.prove(["pm.source_grounding"], "native_number_source_grounding", grounding)
        def bindings():
            refs, invocation = self.command(root, ["refs", "C01", "--json"])
            nodes, _ = node_source(root)
            expected = {node["id"] for node in nodes if isinstance(node.get("evidence"), list) and "C01" in node["evidence"]}
            actual = {row["id"] for row in refs["structured"] if row["field"] == "evidence"}
            require(actual == expected and expected, "Binding read differs from actual native source evidence fields")
            report, checked = self.command(root, ["check", str(root), "--json"], explicit_root=False)
            return {"invocation": invocation, "check_invocation": checked, "source_evidence_nodes": sorted(expected), "actual_evidence_nodes": sorted(actual), "report": report}
        self.prove(["compiler.validate_bindings"], "native_binding_validation", bindings)
        read_ids = [identity for identity in self.rows if self.rows[identity]["access_policy"] == "cli-required" and not self.rows[identity]["proofs"] and (identity.startswith("reader.") or identity in ("pm.read_state_ids", "pm.read_briefing", "pm.read_full_sessions", "pm.read_observations", "pm.read_reasoning", "pm.taste_target_read", "compiler.read_coverage_state", "compiler.validate_structure"))]
        self.prove(read_ids, "all_native_source_reads", full_reads)
        def report_stats():
            value, _ = self.command(root, ["status", "--json"])
            files = source_files(root)
            require(value.get("artifact_location") == str(root.resolve()), "Stats artifact location differs from actual selected root")
            require(value.get("file_count") == len(files), "Stats omitted artifact files")
            require(value.get("total_bytes") == sum(len(content) for content in files.values()), "Stats total bytes not grounded in actual files")
            reported = value.get("files")
            require(isinstance(reported, list), "Stats do not expose every file and exact byte size")
            require({row["path"]: row["bytes"] for row in reported} == {path: len(content) for path, content in files.items()},
                    "Stats per-file sizes differ from actual artifact bytes")
            validated, invocation = self.command(root, ["check", str(root), "--json"], explicit_root=False)
            return {"status": value, "observed_check": validated, "check_invocation": invocation,
                    "seal_claim": False, "scope": "Only the actually invoked structural check; no semantic/model fidelity claim"}
        if "compiler.report_stats" in self.rows:
            self.prove(["compiler.report_stats"], "observed_compiler_stats", report_stats)
        def coverage_repair():
            document = "logic/solution/domain-specific.md"
            before = self.source(root, document, ("Caller section",))
            after = before + "\nCaller source-grounded coverage-gap repair.\n\n" + FULL_TEXT
            paper_before = self.source(root, "PAPER.md")
            paper_body = self.source(root, "PAPER.md", ("Layer Index",))
            paper_after_body = paper_body + "\nCoverage now includes the supplied domain-specific body.\n"
            turn = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["session"]["turn_count"] + 1
            self.apply(root, [{"op": "logic.revise", "target": {"document": document, "heading": ["Caller section"]},
                "set": {"Body": after}, "expected": sha(before.encode()), "session": SESSION, "turn": turn,
                "signal": "user-directive", "provenance": "user"},
                {"op": "paper.edit", "frontmatter": {"claims_summary": ["C01", "C21"]},
                 "heading": ["Layer Index"], "expected": sha(paper_body.encode()), "content": paper_after_body,
                 "audit": {"session": SESSION, "turn": turn, "signal": "user-directive", "provenance": "user",
                           "note": "Caller coverage-repair pass; complete root before/after retained"}},
                {"op": "node.add", "id": "N50", "type": "question", "parent": "root",
                 "title": "Synthetic source-supported coverage gap", "fields": node_fields("question", FULL_TEXT)},
                {"op": "session.log", "session": SESSION, "timestamp": "2026-10-01T10:11",
                 "events": [{"type": "question", "id": "N50", "routing": "direct",
                             "provenance": "ai-executed", "summary": "Caller supplied source-supported gap"}]}])
            require(after in self.source(root, document), "Coverage repair lost original domain prose")
            paper_after = self.source(root, "PAPER.md")
            history = yaml_load(root / f"trace/sessions/{SESSION}.yaml")["logic_revisions"]
            actual_after = self.source(root, document, ("Caller section",))
            require(any(row["turn"] == turn and row["field"] == "Body" and row["before"] == before
                        and row["after"] == actual_after for row in history), "Logic coverage repair lacks exact final-source endpoint history")
            require(any(row["turn"] == turn and row["entry"] == "PAPER.md" and row["field"] == "document"
                        and row["before"] == paper_before and row["after"] == paper_after for row in history),
                    "Root coverage repair lacks exact full PAPER history")
            self.command(root, ["validate", str(root), "--json"], explicit_root=False)
            return {"before": before, "after": after, "paper_before": paper_before,
                    "paper_after": paper_after, "source_input": "acceptance-input.json"}
        self.prove(["compiler.coverage_repair"], "compiler_coverage_repair", coverage_repair)

    def search_preflight(self) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]] | None:
        """Freeze externally reviewed bytes/criteria before any label measurement."""
        audit: dict[str, Any] = {"measurement_status": "blocked/unmeasured",
                                 "approval_scope": "Bounded functional gate only; no inferred human/upstream/scientific approval"}
        self.evidence["search_preflight"] = audit
        def blocked(reason: str) -> None:
            audit["reason"] = reason
            self.evidence["blockers"].append("Search not measured: " + reason)
            self.evidence["search_measurements"] = {"measurement_status": "blocked/unmeasured",
                                                  "relevance": None, "pairs": None, "reason": reason}
        if self.args.search_approval is None:
            blocked("Independent frozen search criteria are missing; proposed labels/thresholds are not approval")
            return None
        try:
            criteria_bytes = self.args.search_approval.read_bytes()
            audit["criteria_sha256"] = sha(criteria_bytes)
            criteria = json.loads(criteria_bytes)
            require(isinstance(criteria, dict), "Search criteria must be one JSON object")
            reviewer = criteria.get("reviewer")
            require(criteria.get("status") == "approved" and isinstance(reviewer, str) and reviewer.strip()
                    and criteria.get("corpus_revision") == PIN,
                    "Search decision must name independent reviewer and approve the exact corpus pin")
            for split in ("development", "heldout"):
                require(isinstance(criteria.get(split), dict), "Frozen numeric criteria missing for " + split)
                for key in ("recall_at_10", "duplicate_precision", "duplicate_recall"):
                    value = criteria[split].get(key)
                    require(isinstance(value, (int, float)) and not isinstance(value, bool)
                            and 0 <= value <= 1 and math.isfinite(value),
                            "Frozen criterion must be a finite fraction: " + split + "." + key)
            relevance_bytes = (self.args.fixture / "search/relevance.json").read_bytes()
            pair_bytes = (self.args.fixture / "search/pairs.json").read_bytes()
            hashes = {"relevance_sha256": sha(relevance_bytes), "pairs_sha256": sha(pair_bytes)}
            audit.update(hashes)
            require(all(criteria.get(key) == digest for key, digest in hashes.items()),
                    "Search approval does not freeze exact label bytes")
            # Parse and use label contents only after the review/hash binding passes.
            labels, pairs = json.loads(relevance_bytes), json.loads(pair_bytes)
            require(labels["corpus_revision"] == pairs["corpus_revision"] == PIN,
                    "Search label contents do not match the reviewed artifact pin")
            require(reviewer not in (labels.get("annotation_author"), pairs.get("annotation_author")),
                    "Label author cannot supply the independent criteria review")
            natural_positives = any(isinstance(row["right"], str) and row["label"] == "duplicate"
                                    for split in ("development", "heldout") for row in pairs[split])
            require(natural_positives or criteria.get("allows_crafted_controls") is True,
                    "Crafted-only duplicate positives need an explicit bounded-control approval")
        except (AcceptanceFailure, OSError, ValueError, KeyError, TypeError) as error:
            blocked(str(error))
            return None
        audit.update({"measurement_status": "frozen_before_measurement", "reviewer": reviewer,
                      "decision": criteria["status"], "frozen_before_measurement": True,
                      "criteria": criteria, "relevance": hashes["relevance_sha256"], "pairs": hashes["pairs_sha256"]})
        return criteria, labels, pairs

    def search(self) -> dict[str, Any]:
        frozen = self.search_preflight()
        if frozen is None:
            return {"measurement_status": "blocked/unmeasured", "relevance": None, "pairs": None,
                    "preflight": self.evidence["search_preflight"], "generalization_or_scientific_claim": False}
        criteria, labels, pairs = frozen
        evaluation = {}
        for split in ("development", "heldout"):
            queries = []
            for row in labels[split]:
                def evaluate(row=row):
                    value, index = self.command(self.args.fixture, ["find", row["query"], "--limit", "10", "--json"])
                    require(all(isinstance(result.get("score"), (int, float)) and math.isfinite(result["score"])
                                for result in value["results"]), "Keyword search returned a missing/non-finite score")
                    returned = [r.get("id", r.get("key")) for r in value["results"]]
                    relevant = set(row["relevant_ids"])
                    matched = relevant.intersection(returned)
                    for identity in returned:
                        shown, _ = self.command(self.args.fixture, ["show", identity, "--full", "--json"])
                        require(shown["entries"][0].get("id", shown["entries"][0].get("key")) == identity, "Ranked source identity cannot be reopened")
                    return {**row, "returned": returned, "recall_at_10": len(matched) / len(relevant),
                            "precision_at_10": len(matched) / 10,
                            "labeled_precision_among_returned": len(matched) / len(returned) if returned else 0,
                            "label_completeness": "Curated acceptable targets; unlabeled results are not independently judged negatives",
                            "misses": sorted(relevant - set(returned)), "unlabeled_returned": sorted(set(returned) - relevant), "invocation": index}
                name = f"search.{split}.{len(queries)}"
                self.scenario(name, evaluate)
                queries.append(self.evidence["sections"][name])
            measured = [q["data"] for q in queries if q["passed"]]
            evaluation[split] = {"queries": queries, "macro_recall_at_10": statistics.mean(q["recall_at_10"] for q in measured) if measured else None,
                                 "macro_precision_at_10": statistics.mean(q["precision_at_10"] for q in measured) if measured else None, "partial": len(measured) != len(queries)}
        # Observe duplicate warnings through real writes, not a Python reimplementation of scoring.
        source_nodes, _ = node_source(self.args.fixture); by_id = {n["id"]: n for n in source_nodes}
        pair_evaluation = {}
        for split in ("development", "heldout"):
            outcomes = []
            for ordinal, row in enumerate(pairs[split]):
                name = f"pairs.{split}.{ordinal}"
                def pair(row=row, split=split, ordinal=ordinal):
                    root = self.workspace / f"pair-{split}-{ordinal}"
                    shutil.copytree(self.args.fixture, root)
                    right_id = row["right"] if isinstance(row["right"], str) else row["right"]["derived_from"]
                    right = by_id[right_id]
                    identity = "N999999"
                    fields = {k: v for k, v in right.items()
                              if k not in ("id", "type", "title", "children", "parent", "also_depends_on", "same_as")}
                    # This is the declared title/substantive-body replay transform;
                    # structural links are not ranking text. All substantive fields,
                    # including evidence prose/pointers, remain literal at the real pin.
                    result = self.apply(root, [{"op": "node.add", "id": identity,
                        "type": right["type"], "parent": "root", "title": right["title"], "fields": fields}])
                    candidates = result.get("duplicate_candidates", [])
                    if not candidates:
                        candidates = [candidate for op in result.get("operations", []) for candidate in op.get("duplicate_candidates", [])]
                    require("duplicate_candidates" in result or any("duplicate_candidates" in op for op in result.get("operations", [])), "apply does not expose observable duplicate advisory candidates")
                    predicted = any(c.get("id") == row["left"] or
                                    {c.get("left"), c.get("right")} == {row["left"], identity}
                                    for c in candidates)
                    actual, _ = node_source(root)
                    require({n["id"] for n in actual} == set(by_id) | {identity}, "Duplicate advisory changed entry survival")
                    return {**row, "predicted_duplicate": predicted, "expected_duplicate": row["label"] == "duplicate",
                            "candidates": candidates, "crafted_positive": isinstance(row["right"], dict),
                            "ranking_corpus": "complete unmodified pinned artifact before one append"}
                self.scenario(name, pair)
                outcomes.append(self.evidence["sections"][name])
            measured = [o["data"] for o in outcomes if o["passed"]]
            tp = sum(o["predicted_duplicate"] and o["expected_duplicate"] for o in measured)
            fp = sum(o["predicted_duplicate"] and not o["expected_duplicate"] for o in measured)
            fn = sum(not o["predicted_duplicate"] and o["expected_duplicate"] for o in measured)
            pair_evaluation[split] = {"pairs": outcomes, "tp": tp, "fp": fp, "fn": fn, "precision": tp / (tp + fp) if tp + fp else None, "recall": tp / (tp + fn) if tp + fn else None,
                                      "misses": [o for o in measured if o["expected_duplicate"] and not o["predicted_duplicate"]], "false_warnings": [o for o in measured if o["predicted_duplicate"] and not o["expected_duplicate"]], "partial": len(measured) != len(outcomes)}
        self.evidence["search_measurements"] = {"relevance": evaluation, "pairs": pair_evaluation,
            "label_relevance_sha256": self.evidence["search_preflight"]["relevance_sha256"],
            "label_pairs_sha256": self.evidence["search_preflight"]["pairs_sha256"],
            "criteria_sha256": self.evidence["search_preflight"]["criteria_sha256"],
            "measurement_status": "measured_after_freeze"}
        for split in ("development", "heldout"):
            require(not evaluation[split]["partial"] and evaluation[split]["macro_recall_at_10"] >= criteria[split]["recall_at_10"], "Approved search recall gate missed: " + split)
            stats = pair_evaluation[split]
            require(not stats["partial"] and stats["precision"] is not None and stats["recall"] is not None and stats["precision"] >= criteria[split]["duplicate_precision"] and stats["recall"] >= criteria[split]["duplicate_recall"], "Approved duplicate precision/recall gate missed: " + split)
        # Real nested-directory discovery uses the same pinned source, not a substitute.
        nested = self.args.fixture / "logic/solution"
        self.command(self.args.fixture, ["find", labels["development"][0]["query"], "--limit", "10", "--json"], cwd=nested, explicit_root=False)
        return {"relevance": evaluation, "pairs": pair_evaluation, "criteria": criteria,
                "proposed_criteria": {"relevance": labels["proposed_acceptance"], "pairs": pairs["proposed_acceptance"]},
                "limits": pairs["limitations"], "generalization_or_scientific_claim": False}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, required=True, help="Actual already-built release ara; never built by this runner")
    parser.add_argument("--output", type=Path, required=True, help="External JSON evidence path, outside source/corpus trees")
    parser.add_argument("--fixture", type=Path, default=ROOT / "crates/ara-core/tests/fixtures/agent-cli")
    parser.add_argument("--corpus", type=Path, default=ROOT.parent / "ara-paperbench/artifacts")
    parser.add_argument("--inventory", type=Path, default=ROOT.parent / "Agent-Native-Research-Artifact/evaluation/agent-cli/operation-coverage.json")
    parser.add_argument("--sections", nargs="+", choices=SECTIONS, default=list(SECTIONS))
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=30)
    parser.add_argument("--runner", default=platform.node(), help="Fixed performance runner label recorded verbatim")
    parser.add_argument("--toolchain", required=True, help="Actual release build toolchain identity supplied by builder; no inference from a prebuilt binary")
    parser.add_argument("--search-approval", type=Path, help="Independently reviewed frozen label hashes and quality criteria JSON")
    args = parser.parse_args()
    if args.repeats < 3 or args.timeout <= 0:
        parser.error("At least three measured repeats and a positive explicit timeout are required")
    for field in ("binary", "output", "fixture", "corpus", "inventory"):
        setattr(args, field, getattr(args, field).expanduser().resolve())
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error("--binary must name an executable prebuilt release binary")
    if "debug" in args.binary.parts:
        parser.error("Debug binaries cannot establish release timing acceptance")
    for source in (ROOT, args.fixture, args.corpus, args.inventory.parent):
        if args.output == source or source in args.output.parents:
            parser.error("--output must be outside all repository/fixture/corpus source trees")
    return args


def main() -> int:
    args = parse_args()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="ara-cli-acceptance-") as temporary:
        runner = Runner(args, Path(temporary))
        runner.evidence["environment"]["rust_toolchain_source"] = (ROOT / "rust-toolchain.toml").read_text()
        hardware_command = ["sysctl", "-n", "hw.model", "hw.memsize", "machdep.cpu.brand_string"] if sys.platform == "darwin" else ["uname", "-a"]
        try:
            hardware = subprocess.run(hardware_command, capture_output=True, text=True, timeout=5)
            runner.evidence["environment"]["hardware_probe"] = {"argv": hardware_command, "exit": hardware.returncode, "stdout": hardware.stdout, "stderr": hardware.stderr}
        except (OSError, subprocess.TimeoutExpired) as error:
            runner.evidence["environment"]["hardware_probe"] = {"unavailable": str(error)}
        try:
            import yaml
            runner.evidence["environment"]["pyyaml"] = yaml.__version__
        except ImportError:
            runner.evidence["failures"].append({"scenario": "prerequisite", "message": "Offline PyYAML prerequisite missing; install it separately before acceptance"})
        else:
            for section in dict.fromkeys(args.sections):
                runner.scenario(section, getattr(runner, section))
                # Persist evidence even if a later section fails or the runner is interrupted.
                args.output.write_text(json.dumps(runner.evidence, ensure_ascii=False, indent=2) + "\n")
        runner.evidence["acceptance_complete"] = set(args.sections) == set(SECTIONS)
        runner.evidence["passed"] = not runner.evidence["failures"] and not runner.evidence["blockers"]
        runner.evidence["temporary_artifacts"] = "Removed after run; exact invocation outputs and required source/input content retained in JSON evidence"
        args.output.write_text(json.dumps(runner.evidence, ensure_ascii=False, indent=2) + "\n")
        print(dump({"evidence": str(args.output), "passed": runner.evidence["passed"], "acceptance_complete": runner.evidence["acceptance_complete"], "failures": len(runner.evidence["failures"]), "blockers": runner.evidence["blockers"]}))
        return 0 if runner.evidence["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
