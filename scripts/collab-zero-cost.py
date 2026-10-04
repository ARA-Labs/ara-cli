#!/usr/bin/env python3
"""Zero-cost measurement harness for the collaborative-research series.

Implements the frozen policy in docs/collaborative-research/zero-cost-measurement.md.
Two already-built release binaries are compared on generated fixtures:

  python3 scripts/collab-zero-cost.py --parent-bin parent/ara --candidate-bin target/release/ara \\
      --anra ../Agent-Native-Research-Artifact --out zero-cost.json

Standard library only. Peak RSS uses /usr/bin/time (-l on macOS, -v on Linux), which
must be allowed to run. `--quick` (1 warm-up, 3 measured, 2 RSS runs) only smoke-tests
the harness; its output is never gate-valid. Exit 0 = pass, 1 = regression or check
failure, 2 = harness error. Clean build time and the wasm check are not run here.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import logging
import math
import os
import platform
import random
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

LOG = logging.getLogger("collab-zero-cost")
PINNED_ANRA = "03f19c7767ec993ae53a0698417b8b68040d7fee"
SEED = 20261003
MIB = 1 << 20
SOURCE_KEY = "zero-cost-source"
POLICY = Path(__file__).resolve().parent.parent / "docs/collaborative-research/zero-cost-measurement.md"
STATES = ("S0", "S1", "S2", "S3")
NORMALIZATION = [
    "artifact_location values replaced by <removed>",
    "absolute fixture/input paths (and their realpaths) replaced by placeholders",
    "ISO-8601 date-time strings replaced by <ts>",
    "binary version strings replaced by <version>",
    "merge report phase clocks (\"*_ms\" numbers) replaced by <ms> (not named by the policy; "
    "they are wall clocks and differ on every run)",
]
TS_RE = re.compile(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?")
LOC_RE = re.compile(r'("artifact_location"\s*:\s*)"(?:[^"\\]|\\.)*"')
MS_RE = re.compile(r'("[A-Za-z_]+_ms"\s*:\s*)-?\d[\d.eE+-]*')


class HarnessError(Exception):
    """The harness itself could not run; maps to exit code 2."""


@dataclass(frozen=True)
class Reps:
    """Repetition counts for one measurement round."""

    warmup: int
    measured: int
    rss: int


@dataclass
class Case:
    """One measured command on one fixture under one store state."""

    command: str
    kind: str  # read | write | merge
    fixture: str
    state: str
    args: List[str]  # "{dir}" is replaced with the target artifact root
    source: Path  # fixture root (reads) or S0 root that is cloned per invocation
    labels: Dict[str, str] = field(default_factory=dict)  # extra path -> placeholder

    @property
    def key(self) -> str:
        return f"{self.command} | {self.fixture} | {self.state}"


@dataclass
class Sample:
    """Outcome of one child invocation."""

    ns: int
    digest: str
    exit: int
    excerpt: str
    violations: List[str]
    rss: Optional[int] = None


def run(argv: List[str], cwd: Optional[Path] = None, check: bool = True) -> subprocess.CompletedProcess:
    """Run a helper command (not measured) and return its completed process."""
    proc = subprocess.run(argv, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise HarnessError(f"{' '.join(map(str, argv))} failed ({proc.returncode}): {proc.stderr.strip()[:500]}")
    return proc


def sha256_file(path: Path) -> str:
    """Return the hex SHA-256 of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(MIB), b""):
            digest.update(chunk)
    return digest.hexdigest()


def clone_tree(src: Path, dst: Path) -> None:
    """Copy a directory tree, using copy-on-write clones where the platform supports them."""
    dst.parent.mkdir(parents=True, exist_ok=True)
    flags = {"Darwin": ["-cRp"], "Linux": ["-a", "--reflink=auto"]}.get(platform.system())
    if flags and subprocess.run(["cp", *flags, str(src), str(dst)], capture_output=True).returncode == 0:
        return
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(src, dst, symlinks=True)


def unlock_tree(root: Path) -> None:
    """Restore S2 permissions (chmod 700 on every mode-000 .ara/vcs) under root."""
    for dirpath, dirnames, _ in os.walk(root):
        if os.path.basename(dirpath) == ".ara" and "vcs" in dirnames:
            vcs = os.path.join(dirpath, "vcs")
            if not os.access(vcs, os.R_OK | os.X_OK):
                os.chmod(vcs, 0o700)


def remove_tree(root: Path) -> None:
    """Delete a tree after restoring S2 permissions."""
    if root.exists():
        unlock_tree(root)
        shutil.rmtree(root)


def percentile(values: List[int], q: float) -> int:
    """Nearest-rank percentile."""
    ordered = sorted(values)
    return ordered[max(0, math.ceil(q * len(ordered)) - 1)]


def ara_listing(root: Path) -> Dict[str, Tuple[str, int, int]]:
    """List .ara/ recursively as rel -> (kind, size, mode); unreadable dirs are not descended."""
    base = root / ".ara"
    result: Dict[str, Tuple[str, int, int]] = {}
    if not os.path.lexists(base):
        return result
    pending = [base]
    while pending:
        current = pending.pop()
        info = os.lstat(current)
        rel = os.path.relpath(current, base)
        is_dir = stat.S_ISDIR(info.st_mode)
        # Directory sizes are filesystem bookkeeping; names already capture their content.
        result[rel] = ("dir" if is_dir else "file", -1 if is_dir else info.st_size, stat.S_IMODE(info.st_mode))
        if is_dir and os.access(current, os.R_OK | os.X_OK):
            pending.extend(Path(current, name) for name in os.listdir(current))
    return result


def forbidden_paths(container: Path) -> set:
    """Find snapshot.json files and .ara-snapshot- staging dirs (skipping .ara/vcs contents)."""
    found = set()
    for dirpath, dirnames, filenames in os.walk(container):
        if os.path.basename(dirpath) == ".ara":
            dirnames[:] = [d for d in dirnames if d != "vcs"]
        for name in dirnames + filenames:
            if name == "snapshot.json" or name.startswith(".ara-snapshot-"):
                found.add(os.path.relpath(os.path.join(dirpath, name), container))
    return found


def access_violations(case: Case, before: dict, after: dict, fb: set, fa: set, root: Path) -> List[str]:
    """Apply the policy's store-absence rules to one invocation."""
    problems = []
    changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
    for rel in changed:
        allowed = case.kind != "read" and (rel in (".", "lock") or rel == "transactions" or rel.startswith("transactions/"))
        if not allowed:
            problems.append(f".ara/{rel} changed: {before.get(rel)} -> {after.get(rel)}")
    problems += [f"created {path}" for path in sorted(fa - fb)]
    if case.state == "S0" and os.path.lexists(root / ".ara" / "vcs"):
        problems.append("created .ara/vcs/")
    return problems


class Harness:
    """Builds fixtures, runs every case, and assembles the JSON record."""

    def __init__(self, args: argparse.Namespace, work: Path) -> None:
        self.args = args
        self.work = work
        self.bins = {"parent": str(Path(args.parent_bin).resolve()), "candidate": str(Path(args.candidate_bin).resolve())}
        self.reps = Reps(1, 3, 2) if args.quick else Reps(3, 15, 5)
        self.rng = random.Random(SEED)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("ARA_")}
        self.versions = {side: run([b, "--version"]).stdout.strip() for side, b in self.bins.items()}
        self.version_res = [re.compile(r"(?<![\w.])" + re.escape(v.split()[-1]) + r"(?![\w.])") for v in self.versions.values()]
        self.fixtures: Dict[str, Path] = {}
        self.info: Dict[str, object] = {}
        self.infeasible: List[dict] = []
        self.deviations: List[str] = []
        self.counter = 0

    # ---------- fixtures ----------
    def write_random(self, directory: Path, count: int, size: int, prefix: str) -> None:
        """Write seeded random files (one shared RNG, fixed generation order)."""
        directory.mkdir(parents=True, exist_ok=True)
        for index in range(count):
            (directory / f"{prefix}-{index:04d}.bin").write_bytes(self.rng.randbytes(size))

    def apply_state(self, root: Path, state: str) -> None:
        """Materialize store state S0-S3 inside an artifact root."""
        vcs = root / ".ara" / "vcs"
        if state in ("S1", "S3"):
            clone_tree(self.work / "state" / state, vcs)
        elif state == "S2":
            vcs.mkdir(parents=True)
            (vcs / "HEAD").write_text("unreadable\n")
            os.chmod(vcs, 0)

    def materialize(self, source: Path, state: str, dest: Path) -> Path:
        """Clone an S0 root to dest/ara and apply a store state; returns the new root."""
        root = dest / "ara"
        clone_tree(source, root)
        self.apply_state(root, state)
        return root

    def build_fixtures(self) -> None:
        """Generate F1, F2, store-state masters, and the F3 import history."""
        anra = Path(self.args.anra).resolve()
        example = anra / "examples" / "the-ara-of-ara"
        if not (example / "trace" / "exploration_tree.yaml").is_file():
            raise HarnessError(f"F1 source not found: {example}")
        commit = run(["git", "-C", str(anra), "rev-parse", "HEAD"]).stdout.strip()
        dirty = run(["git", "-C", str(anra), "status", "--porcelain", "--", "examples/the-ara-of-ara"]).stdout.strip()
        if commit != PINNED_ANRA:
            LOG.warning("Agent-Native-Research-Artifact is at %s, not pinned %s", commit, PINNED_ANRA)
        if dirty:
            LOG.warning("examples/the-ara-of-ara has local modifications")
        self.info["anra"] = {"path": str(anra), "commit": commit, "pinned_commit": PINNED_ANRA,
                             "pinned": commit == PINNED_ANRA, "example_clean": not dirty}
        f1 = self.work / "base" / "F1"
        clone_tree(example, f1)
        f2 = self.work / "base" / "F2"
        clone_tree(f1, f2)
        self.write_random(f2 / "evidence" / "blobs", 256, MIB, "blob")
        self.write_random(f2 / "src" / "blobs", 64, 256 * 1024, "blob")
        self.write_random(self.work / "state" / "S1", 64, MIB, "garbage")
        for index in range(4096):
            path = self.work / "state" / "S3" / f"{index % 64:02x}" / f"obj-{index:04d}"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(self.rng.randbytes(64))
        for name, source in (("F1", f1), ("F2", f2)):
            for state in STATES:
                self.fixtures[f"{name}-{state}"] = self.materialize(source, state, self.work / "fx" / f"{name}-{state}")
        self.base = {"F1": f1, "F2": f2}
        self.build_history()

    def synthetic_seed(self) -> Path:
        """Fixture M: a deterministic artifact that ara 0.1.23 can merge.

        F1 carries session-index and promotion diagnostics that the merger
        rejects, so merge cases and F3 use M instead (policy: Fixtures).
        """
        root = self.work / "base" / "M"
        nodes = "".join(
            f"  - id: N{index:02d}\n    type: question\n    title: Seed question {index}\n"
            f"    description: Deterministic merge fixture node {index}.\n"
            for index in range(1, 21)
        )
        claims = "".join(
            f"\n## C{index:02d}: Seed claim {index}\n- **Statement**: Fixture statement {index}.\n"
            f"- **Status**: hypothesis\n"
            for index in range(1, 11)
        )
        for relative, text in (("trace/exploration_tree.yaml", "tree:\n" + nodes),
                               ("logic/claims.md", "# Claims\n" + claims),
                               ("src/train.py", "print('seed')\n")):
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.write_random(root / "evidence", 4, 64 * 1024, "seed")
        return root

    def root_node(self, root: Path) -> str:
        """First node ID in the exploration tree."""
        match = re.search(r'(?m)^\s*-?\s*id:\s*"?(N\d+)', (root / "trace" / "exploration_tree.yaml").read_text())
        if not match:
            raise HarnessError(f"no node in {root}")
        return match.group(1)

    def build_history(self) -> None:
        """Build 41 linear source revisions and F3 (40 imports) with the parent binary."""
        seed = Path(self.args.merge_seed).resolve() if self.args.merge_seed else self.synthetic_seed()
        if self.args.merge_seed:
            self.deviations.append(f"--merge-seed {seed}: merge fixtures and F3 use this seed instead of F1")
        self.merge_seed = seed
        parent_node = self.root_node(seed)
        hist = self.work / "history"
        self.revisions = [hist / "src-00" / "ara"]
        clone_tree(seed, self.revisions[0])
        for index in range(1, 42):
            current = hist / f"src-{index:02d}" / "ara"
            clone_tree(self.revisions[-1], current)
            had_ignore = (current / ".gitignore").exists()
            run([self.bins["parent"], "-C", str(current), "add", "node", "--type", "question", "--parent", parent_node,
                 "--title", f"Imported probe {index}", "--set", f"description=Seeded import node {index}",
                 "--provenance", "user", "--no-duplicate-check", "--json"])
            shutil.rmtree(current / ".ara")  # writer bookkeeping is not source content
            if not had_ignore:
                (current / ".gitignore").unlink(missing_ok=True)
            self.revisions.append(current)
        dest = self.work / "fx" / "F3-S0" / "ara"
        clone_tree(seed, dest)
        for index in range(1, 41):
            proc = run([self.bins["parent"], "-C", str(dest), "merge", "--base", str(self.revisions[index - 1]),
                        "--theirs", str(self.revisions[index]), "--source-key", SOURCE_KEY, "--as", SOURCE_KEY,
                        "--json"], check=False)
            if proc.returncode != 0:
                message = proc.stderr or proc.stdout
                match = re.search(r'"message"\s*:\s*"((?:[^"\\]|\\.)*)"', message)
                self.infeasible.append({"item": "F3 and the 41st-import merge", "import": index, "exit": proc.returncode,
                                        "reason": match.group(1) if match else message[:300]})
                LOG.warning("F3 import %d rejected by the parent binary; F3 cases are skipped", index)
                return
        shutil.rmtree(dest / ".ara", ignore_errors=True)
        self.fixtures["F3-S0"] = dest

    # ---------- cases ----------
    def cases(self) -> List[Case]:
        """Enumerate every read, write, and merge case."""
        claims = json.loads(run([self.bins["parent"], "-C", str(self.base["F1"]), "ls", "--type", "claim", "--json"]).stdout)
        claim = claims["entries"][0]["id"]
        node = self.root_node(self.base["F1"])
        inputs = self.work / "inputs"
        inputs.mkdir(exist_ok=True)
        batch = inputs / "batch.jsonl"
        batch.write_text(json.dumps({"op": "node.add", "id": "$probe", "type": "question", "parent": node,
                                     "title": "Zero-cost apply probe",
                                     "fields": {"description": "Fixed batch payload", "provenance": "user"}}) + "\n")
        reads = [
            ("status --json", ["-C", "{dir}", "status", "--json"]),
            ("ls --json", ["-C", "{dir}", "ls", "--json"]),
            (f"show {claim} --json", ["-C", "{dir}", "show", claim, "--json"]),
            ('find "method" --json', ["-C", "{dir}", "find", "method", "--json"]),
            ("validate <dir> --json", ["validate", "{dir}", "--json"]),
            ("check <dir>", ["check", "{dir}"]),
        ]
        result = []
        for name, root in self.fixtures.items():
            fixture, state = name.split("-")
            result += [Case(c, "read", fixture, state, a, root) for c, a in reads]
        writes = [
            ("add node", ["-C", "{dir}", "add", "node", "--type", "question", "--parent", node, "--title",
                          "Zero-cost add probe", "--set", "description=Fixed add payload", "--provenance", "user", "--json"]),
            ("apply", ["-C", "{dir}", "apply", str(batch), "--json"]),
        ]
        for state in STATES:
            result += [Case(c, "write", "F1", state, a, self.base["F1"]) for c, a in writes]
        def merge_args(base: Path, theirs: Path) -> List[str]:
            return ["-C", "{dir}", "merge", "--base", str(base), "--theirs", str(theirs),
                    "--source-key", SOURCE_KEY, "--as", SOURCE_KEY, "--json"]
        labels = {str(self.revisions[0]): "<base>", str(self.revisions[1]): "<theirs>"}
        seed_name = "merge-seed" if self.args.merge_seed else "M"
        for state in STATES:
            result.append(Case("merge first import", "merge", seed_name, state,
                               merge_args(self.revisions[0], self.revisions[1]), self.merge_seed, labels))
        if "F3-S0" in self.fixtures:
            labels = {str(self.revisions[40]): "<base>", str(self.revisions[41]): "<theirs>"}
            result.append(Case("merge 41st import", "merge", "F3", "S0",
                               merge_args(self.revisions[40], self.revisions[41]), self.fixtures["F3-S0"], labels))
        return result

    # ---------- invocation ----------
    def normalize(self, text: str, labels: Dict[str, str]) -> str:
        """Apply the policy's output normalization."""
        text = LOC_RE.sub(r'\1"<removed>"', text)
        pairs = []
        for path, label in labels.items():
            pairs += [(os.path.realpath(path), label), (path, label)]
        for path, label in sorted(pairs, key=lambda p: -len(p[0])):
            text = text.replace(path, label)
        text = TS_RE.sub("<ts>", text)
        for pattern in self.version_res:
            text = pattern.sub("<version>", text)
        return MS_RE.sub(r"\1<ms>", text)

    def invoke(self, side: str, case: Case, rss: bool = False) -> Sample:
        """Run one child process for a case and capture timing, output, and access results."""
        scratch = None
        if case.kind == "read":
            root = case.source
        else:
            self.counter += 1
            scratch = self.work / "run" / f"{self.counter:06d}"
            root = self.materialize(case.source, case.state, scratch)
        container = root.parent
        before, fb = ara_listing(root), forbidden_paths(container)
        argv = [self.bins[side]] + [a.replace("{dir}", str(root)) for a in case.args]
        timefile = self.work / "time.txt"
        if rss:
            argv = ["/usr/bin/time", "-l" if platform.system() == "Darwin" else "-v", "-o", str(timefile)] + argv
        start = time.perf_counter_ns()
        proc = subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, env=self.env, cwd=self.work)
        elapsed = time.perf_counter_ns() - start
        after, fa = ara_listing(root), forbidden_paths(container)
        violations = access_violations(case, before, after, fb, fa, root)
        labels = {str(root): "<root>", str(self.work): "<work>", **case.labels}
        out = self.normalize(proc.stdout.decode("utf-8", "replace"), labels)
        err = self.normalize(proc.stderr.decode("utf-8", "replace"), labels)
        digest = hashlib.sha256(f"{proc.returncode}\0{out}\0{err}".encode()).hexdigest()
        sample = Sample(elapsed, digest, proc.returncode, (out + err)[:300], violations)
        if rss:
            sample.rss = self.parse_rss(timefile.read_text())
        if scratch is not None:
            remove_tree(scratch)
        return sample

    @staticmethod
    def parse_rss(text: str) -> int:
        """Extract peak RSS in bytes from /usr/bin/time output."""
        mac = re.search(r"(\d+)\s+maximum resident set size", text)
        if mac:
            return int(mac.group(1))
        linux = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", text)
        if linux:
            return int(linux.group(1)) * 1024
        raise HarnessError(f"/usr/bin/time reported no maximum resident set size: {text.strip()[:300]}")

    # ---------- measurement ----------
    def timing_round(self, case: Case, outputs: Dict[str, List[Sample]]) -> dict:
        """Warm-ups then measured invocations, alternating parent/candidate."""
        times: Dict[str, List[int]] = {"parent": [], "candidate": []}
        first: Dict[str, int] = {}
        for index in range(self.reps.warmup + self.reps.measured):
            for side in ("parent", "candidate"):
                sample = self.invoke(side, case)
                outputs[side].append(sample)
                first.setdefault(side, sample.ns)
                if index >= self.reps.warmup:
                    times[side].append(sample.ns)
        return {side: {"median_ms": percentile(v, 0.5) / 1e6, "p90_ms": percentile(v, 0.9) / 1e6,
                       "first_ms": first[side] / 1e6, "samples_ms": [x / 1e6 for x in v]} for side, v in times.items()}

    def rss_round(self, case: Case, outputs: Dict[str, List[Sample]]) -> dict:
        """Separate alternating invocations under /usr/bin/time; median peak RSS."""
        values: Dict[str, List[int]] = {"parent": [], "candidate": []}
        for _ in range(self.reps.rss):
            for side in ("parent", "candidate"):
                sample = self.invoke(side, case, rss=True)
                outputs[side].append(sample)
                values[side].append(sample.rss or 0)
        return {side: {"rss_bytes": percentile(v, 0.5), "samples_bytes": v} for side, v in values.items()}

    @staticmethod
    def timing_failures(stats: dict) -> List[str]:
        """Metrics exceeding the wall-time tolerances."""
        p, c = stats["parent"], stats["candidate"]
        bad = []
        if c["median_ms"] > p["median_ms"] * 1.10 + 3:
            bad.append("median_wall")
        if c["p90_ms"] > p["p90_ms"] * 1.20 + 5:
            bad.append("p90_wall")
        return bad

    @staticmethod
    def rss_failures(stats: dict) -> List[str]:
        """Metrics exceeding the peak-RSS tolerance."""
        limit = stats["parent"]["rss_bytes"] * 1.05 + MIB
        return ["median_rss"] if stats["candidate"]["rss_bytes"] > limit else []

    def measure(self, case: Case) -> dict:
        """Measure one case, confirm tolerance breaches with a rerun, and check outputs."""
        LOG.info("measuring %s", case.key)
        outputs: Dict[str, List[Sample]] = {"parent": [], "candidate": []}
        timing = [self.timing_round(case, outputs)]
        rss = [self.rss_round(case, outputs)]
        regressions: List[str] = []
        if self.timing_failures(timing[0]):
            timing.append(self.timing_round(case, outputs))
            regressions += sorted(set(self.timing_failures(timing[0])) & set(self.timing_failures(timing[1])))
        if self.rss_failures(rss[0]):
            rss.append(self.rss_round(case, outputs))
            regressions += sorted(set(self.rss_failures(rss[0])) & set(self.rss_failures(rss[1])))
        failures = [f"regression: {name}" for name in regressions]
        digests = {side: sorted({s.digest for s in samples}) for side, samples in outputs.items()}
        for side, values in digests.items():
            if len(values) != 1:
                failures.append(f"{side} output differs between invocations")
        equal = digests["parent"] == digests["candidate"]
        if not equal:
            failures.append("parent and candidate outputs differ")
        violations = sorted({f"{side}: {v}" for side, samples in outputs.items() for s in samples for v in s.violations})
        if violations:
            failures.append("store access check failed")
        summary = {side: {**timing[0][side], "rss_bytes": rss[0][side]["rss_bytes"]} for side in ("parent", "candidate")}
        for side in summary:
            summary[side].pop("samples_ms")
        return {
            "command": case.command, "kind": case.kind, "fixture": case.fixture, "state": case.state,
            "parent": summary["parent"], "candidate": summary["candidate"],
            "timing_rounds": timing, "rss_rounds": rss,
            "verdict": "regression" if regressions else ("fail" if failures else "pass"),
            "output": {"parent_equals_candidate": equal, "digest": digests,
                       "exit": {side: sorted({s.exit for s in samples}) for side, samples in outputs.items()},
                       "excerpt": {side: samples[0].excerpt for side, samples in outputs.items()}},
            "access": {"ok": not violations, "violations": violations[:50]},
            "failures": failures,
        }

    def compare_states(self, results: List[dict]) -> None:
        """Require S1/S2/S3 outputs to equal the S0 output for each binary."""
        index = {(r["command"], r["fixture"], r["state"]): r for r in results}
        for result in results:
            if result["state"] == "S0":
                continue
            s0 = index.get((result["command"], result["fixture"], "S0"))
            check = {side: s0 is not None and result["output"]["digest"][side] == s0["output"]["digest"][side]
                     for side in ("parent", "candidate")}
            result["output"]["equals_S0"] = check
            for side, ok in check.items():
                if not ok:
                    result["failures"].append(f"{side} output under {result['state']} differs from S0")
                    if result["verdict"] == "pass":
                        result["verdict"] = "fail"

    # ---------- build checks ----------
    def build_checks(self) -> dict:
        """Binary size always; dependency tree only when both source checkouts are given."""
        sizes = {side: os.path.getsize(path) for side, path in self.bins.items()}
        limit = sizes["parent"] * 1.02 + 64 * 1024
        checks: dict = {"binary_size": {"parent_bytes": sizes["parent"], "candidate_bytes": sizes["candidate"],
                                        "limit_bytes": limit, "pass": sizes["candidate"] <= limit}}
        if self.args.parent_src and self.args.candidate_src:
            trees = {side: self.dependency_tree(Path(src)) for side, src in
                     (("parent", self.args.parent_src), ("candidate", self.args.candidate_src))}
            names = {side: {line.split()[0] for line in lines} for side, lines in trees.items()}
            new = sorted(names["candidate"] - names["parent"])
            checks["dependency_tree"] = {
                "new_crates": new, "removed_crates": sorted(names["parent"] - names["candidate"]),
                "changed_versions": sorted(set(trees["candidate"]) - set(trees["parent"]) - {l for l in trees["candidate"] if l.split()[0] in new}),
                "pass": not new, "note": "a new crate passes only if the stage plan names it; review new_crates"}
        else:
            checks["dependency_tree"] = {"skipped": "pass --parent-src and --candidate-src to diff cargo tree"}
        checks["not_run"] = ["clean release build time (reported manually)", "wasm cargo check (run manually)"]
        return checks

    @staticmethod
    def dependency_tree(src: Path) -> List[str]:
        """Sorted, deduplicated `cargo tree` lines without path or repeat markers."""
        out = run(["cargo", "tree", "-p", "ara-cli", "-e", "normal", "--prefix", "none"], cwd=src).stdout
        lines = {re.sub(r"\s+\((?:\*|proc-macro|/[^)]*)\)", "", line).strip() for line in out.splitlines()}
        return sorted(line for line in lines if line)

    # ---------- driver ----------
    def execute(self) -> dict:
        """Run the whole policy and return the JSON record."""
        self.build_fixtures()
        results = [self.measure(case) for case in self.cases()]
        self.compare_states(results)
        build = self.build_checks()
        failures = [f"{r['command']} | {r['fixture']} | {r['state']}: {f}" for r in results for f in r["failures"]]
        failures += [f"build: {name}" for name, check in build.items() if isinstance(check, dict) and check.get("pass") is False]
        notes = []
        if self.args.quick:
            notes.append("quick mode: results are NOT gate-valid (harness smoke test only)")
        if self.infeasible:
            notes.append("some policy items could not be executed; see infeasible")
        if self.deviations:
            notes.append("fixture deviations from the policy; see deviations")
        return {
            "format": "ara.collab-zero-cost/v1",
            "timestamp": dt.datetime.now(dt.timezone.utc).isoformat(),
            "quick": bool(self.args.quick), "gate_valid": not notes, "notes": notes,
            "pass": not failures and not self.infeasible, "failures": failures,
            "policy": {"path": str(POLICY), "sha256": sha256_file(POLICY)},
            "binaries": {side: {"path": path, "sha256": sha256_file(Path(path)), "version": self.versions[side]}
                         for side, path in self.bins.items()},
            "machine": {"platform": platform.platform(), "machine": platform.machine(), "cpu_count": os.cpu_count(),
                        "python": platform.python_version()},
            "anra": self.info.get("anra"), "seed": SEED, "repetitions": vars(self.reps),
            "statistics": {"p90": "nearest rank", "first_ms": "first invocation of each binary for the case (a warm-up)"},
            "normalization": NORMALIZATION, "deviations": self.deviations, "infeasible": self.infeasible,
            "fixtures": sorted(self.fixtures), "cases": results, "build": build,
        }


def parse_args(argv: List[str]) -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--parent-bin", required=True)
    parser.add_argument("--candidate-bin", required=True)
    parser.add_argument("--anra", required=True, help="Agent-Native-Research-Artifact checkout")
    parser.add_argument("--out", required=True, help="JSON output path")
    parser.add_argument("--work", help="scratch parent directory (kept); default is a removed temp dir")
    parser.add_argument("--quick", action="store_true", help="1+3 timing and 2 RSS runs; not gate-valid")
    parser.add_argument("--parent-src", help="parent checkout for the cargo tree diff")
    parser.add_argument("--candidate-src", help="candidate checkout for the cargo tree diff")
    parser.add_argument("--merge-seed", help="override the merge/F3 seed artifact (recorded as a deviation)")
    return parser.parse_args(argv)


def main(argv: List[str]) -> int:
    """Entry point: 0 pass, 1 regression/check failure, 2 harness error."""
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s", stream=sys.stderr)
    args = parse_args(argv)
    for name in ("parent_bin", "candidate_bin"):
        if not os.access(getattr(args, name), os.X_OK):
            LOG.error("not an executable: %s", getattr(args, name))
            return 2
    if args.work:
        Path(args.work).mkdir(parents=True, exist_ok=True)
    work = Path(tempfile.mkdtemp(prefix="collab-zero-cost-", dir=args.work)).resolve()
    try:
        record = Harness(args, work).execute()
    except Exception:  # any unexpected failure is a harness error (exit 2), never a verdict
        LOG.exception("harness error")
        return 2
    finally:
        if args.work:
            unlock_tree(work)
        else:
            remove_tree(work)
    Path(args.out).write_text(json.dumps(record, indent=2) + "\n")
    LOG.info("pass=%s gate_valid=%s failures=%d -> %s", record["pass"], record["gate_valid"],
             len(record["failures"]), args.out)
    for failure in record["failures"][:40]:
        LOG.info("FAIL %s", failure)
    return 0 if record["pass"] else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
