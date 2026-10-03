"""Hermetic acceptance-fixture invariants; no ara process, model, or network.

Run after integration: python3 -m unittest discover -s scripts -p 'test_agent_cli_acceptance.py'
"""
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
import unittest


spec = importlib.util.spec_from_file_location("agent_cli_acceptance", Path(__file__).with_name("agent-cli-acceptance.py"))
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


def flatten(document):
    rows, parents = [], {}
    stack = [(row, None) for row in reversed(document["tree"])]
    while stack:
        row, parent = stack.pop()
        rows.append(row)
        parents[row["id"]] = parent
        stack.extend((child, row["id"]) for child in reversed(row.get("children", [])))
    return rows, parents


class AcceptanceGeneratorTests(unittest.TestCase):
    def test_broad_fields_and_references_resolve_in_generated_namespaces(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            metadata = acceptance.generate_artifact(root, 100)
            nodes, parents = flatten(json.loads((root / acceptance.TREE).read_text()))
            self.assertEqual({node["id"] for node in nodes}, {f"N{number:02}" for number in range(1, 101)})
            self.assertEqual(set(parents.values()), {None})
            self.assertEqual({node["type"] for node in nodes}, {"question", "decision", "experiment", "dead_end", "pivot"})
            node_ids = set(parents)
            claims = {f"C{number:02}" for number in range(1, 6)}
            for node in nodes:
                self.assertTrue(set(node.get("also_depends_on", [])).issubset(node_ids - {node["id"]}))
                self.assertEqual(node["provenance"], "ai-executed")
                if node["type"] == "experiment":
                    self.assertTrue(set(node["evidence"]).issubset(claims))
                if node["type"] == "pivot":
                    self.assertEqual(node["from"], "Synthetic initial direction")
                    self.assertEqual(node["to"], f"Synthetic base payload {node['id']}")
                    self.assertEqual(node["trigger"], "Synthetic user directive")
                    self.assertFalse({"prior_direction", "new_direction", "reason"} & node.keys())
            observations = json.loads((root / "staging/observations.yaml").read_text())["observations"]
            self.assertEqual({row["id"] for row in observations}, {f"O{number:02}" for number in range(1, 6)})
            for row in observations:
                self.assertTrue(set(row["bound_to"]).issubset(node_ids))
                self.assertEqual((row["promoted"], row["promoted_to"], row["stale"]), (False, None, False))
            index = json.loads((root / acceptance.INDEX).read_text())["sessions"]
            for row in index:
                session = json.loads((root / f"trace/sessions/{row['id']}.yaml").read_text())
                self.assertEqual(row["turn_count"], session["session"]["turn_count"])
                self.assertEqual(row["events_count"], len(session["events_logged"]))
                for key in ("events_logged", "ai_actions", "claims_touched", "logic_revisions", "key_context"):
                    self.assertEqual([record["turn"] for record in session[key]], list(range(1, row["turn_count"] + 1)))
                self.assertEqual(row["claims_touched"], [record["id"] for record in session["claims_touched"]])
                for event in session["events_logged"]:
                    self.assertIn(event["id"], node_ids)
                    source_node = next(node for node in nodes if node["id"] == event["id"])
                    self.assertEqual(event["type"], source_node["type"])
                    self.assertGreaterEqual(row["date"], source_node["timestamp"][:10])
                for revision in session["logic_revisions"]:
                    self.assertIn(revision["entry"], claims)
                    self.assertEqual(revision["before"], "")
                    self.assertEqual(revision["after"], "Synthetic base statement " + revision["entry"])
            self.assertEqual(metadata["layer_entries_each"], 5)

    def test_deep_shape_is_exact_nested_chain_not_flat_dependency_graph(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            acceptance.generate_artifact(root, 100, "deep")
            nodes, parents = flatten(json.loads((root / acceptance.TREE).read_text()))
            self.assertEqual([node["id"] for node in nodes], [f"N{number:02}" for number in range(1, 101)])
            self.assertEqual(parents["N01"], None)
            for number in range(2, 101):
                self.assertEqual(parents[f"N{number:02}"], f"N{number - 1:02}")
            self.assertEqual(nodes[-1].get("children", []), [])
            self.assertEqual(nodes[-1]["title"], "Synthetic base N100 uniqueneedle")
            self.assertEqual(nodes[-1]["type"], "dead_end")
            self.assertNotIn("dead_end", {node["type"] for node in nodes[:-1]})
            tastes = json.loads((root / "trace/taste_log.yaml").read_text())["entries"]
            by_id = {node["id"]: node for node in nodes}
            for taste in tastes:
                self.assertNotEqual(by_id[taste["target"]]["type"], "question")

    def test_merge_forks_share_base_but_keep_real_colliding_parent_intentions(self):
        with tempfile.TemporaryDirectory() as temporary:
            base, ours, theirs, metadata = acceptance.merge_trio(Path(temporary), 100)
            base_nodes, _ = flatten(json.loads((base / acceptance.TREE).read_text()))
            our_nodes, our_parents = flatten(json.loads((ours / acceptance.TREE).read_text()))
            their_nodes, their_parents = flatten(json.loads((theirs / acceptance.TREE).read_text()))
            self.assertEqual({row["id"] for row in base_nodes}, {f"N{number:02}" for number in range(1, 81)})
            self.assertEqual(set(our_parents), set(their_parents))
            self.assertNotIn("also_depends_on", next(row for row in base_nodes if row["id"] == "N03"))
            for number in range(81, 91):
                identity = f"N{number:02}"
                self.assertEqual(our_parents[identity], "N01")
                self.assertEqual(their_parents[identity], "N02")
                our_node = next(row for row in our_nodes if row["id"] == identity)
                their_node = next(row for row in their_nodes if row["id"] == identity)
                self.assertEqual(our_node["acceptance_extension"]["marker"], "ours")
                self.assertEqual(their_node["acceptance_extension"]["marker"], "theirs")
                if number > 82:
                    self.assertEqual(our_node["also_depends_on"], ["N02"])
                    self.assertEqual(their_node["also_depends_on"], ["N03"])
            # Independent source-only native related-work addresses must not
            # create unintended semantic conflicts in clean timing inputs.
            self.assertIn("## RW05:", (ours / "logic/related_work.md").read_text())
            self.assertIn("## RW06:", (theirs / "logic/related_work.md").read_text())
            self.assertEqual(metadata["merged_nodes"], 100)
            self.assertEqual((base / "src/untouched.txt").read_bytes(), (ours / "src/untouched.txt").read_bytes())
            self.assertEqual((base / "evidence/untouched.txt").read_bytes(), (theirs / "evidence/untouched.txt").read_bytes())

    def test_generation_is_reproducible_and_scaled_history_is_not_constant(self):
        with tempfile.TemporaryDirectory() as temporary:
            first, second, large = (Path(temporary) / name for name in ("first", "second", "large"))
            acceptance.generate_artifact(first, 100)
            acceptance.generate_artifact(second, 100)
            metadata = acceptance.generate_artifact(large, 1000)
            self.assertEqual(acceptance.source_files(first), acceptance.source_files(second))
            self.assertEqual(metadata["layer_entries_each"], 50)
            observations = json.loads((large / "staging/observations.yaml").read_text())["observations"]
            self.assertEqual([row["id"] for row in observations], [f"O{number:02}" for number in range(1, 51)])
            session_rows = sum(len(json.loads(path.read_text()).get("logic_revisions", []))
                               for path in (large / "trace/sessions").glob("*.yaml") if path.name != "session_index.yaml")
            self.assertEqual(session_rows, 50)

    def test_invalid_generator_inputs_fail_before_fixture_creation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "invalid"
            for count, shape in ((0, "broad"), (-1, "broad"), (100, "dependency-only")):
                with self.assertRaises(acceptance.AcceptanceFailure):
                    acceptance.generate_artifact(root, count, shape)
                self.assertFalse(root.exists())

    def test_status_oracle_counts_trace_and_staging_observations_in_their_shared_kind(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            acceptance.save(root, "staging/observations.yaml", "observations: [{id: O01, content: first}, {id: O02, content: second}]\n")
            nodes = [{"id": "N82", "type": "observation"}, {"id": "N01", "type": "question"}]
            self.assertEqual(acceptance.node_and_staging_kind_counts(root, nodes), {"observation": 3, "question": 1})

    def test_historical_replay_reports_all_incompatible_source_without_rewriting(self):
        nodes = [{"id": "N82", "type": "observation", "content": "exact legacy source"},
                 {"id": "N88", "type": "pivot", "prior_direction": "a", "new_direction": "b", "reason": "c"},
                 {"id": "N112", "type": "dead_end", "hypothesis": "a", "failure_mode": "b", "lesson": "c"},
                 {"id": "N113", "type": "question", "description": "d"}]
        original = json.dumps(nodes, sort_keys=True)
        issues = acceptance.replay_incompatibilities(nodes, {"N82": None, "N88": None, "N112": None, "N113": "N112"})
        self.assertEqual([row["id"] for row in issues], ["N82", "N88", "N112"])
        self.assertIn("from, to, trigger", issues[1]["reasons"][0])
        self.assertEqual(issues[2]["children"], ["N113"])
        self.assertEqual(json.dumps(nodes, sort_keys=True), original)

    def test_replay_seeds_only_missing_prerequisites_without_reordering_events(self):
        nodes = [{"id": "N01", "timestamp": "2026-03-09"},
                 {"id": "N02", "timestamp": "2026-03-12"},
                 {"id": "N03", "timestamp": "2026-03-13", "also_depends_on": ["N01"]},
                 {"id": "N04", "timestamp": "2026-03-14"}]
        parents = {"N01": None, "N02": "N01", "N03": None, "N04": None}
        order, inferences = acceptance.replay_sequence(nodes, parents, ["N02", "N03"])
        self.assertEqual(order, ["N01", "N02", "N03", "N04"])
        self.assertEqual([row["id"] for row in inferences], ["N01", "N04"])
        self.assertEqual(inferences[0]["required_before"], "N02")
        self.assertIsNone(inferences[1]["required_before"])
        self.assertEqual({row["status"] for row in inferences}, {"[INFERENCE]"})

    def test_replay_refuses_recorded_order_or_timestamp_contradictions(self):
        nodes = [{"id": "N01", "timestamp": "2026-03-09"},
                 {"id": "N02", "timestamp": "2026-03-12"}]
        parents = {"N01": None, "N02": "N01"}
        with self.assertRaisesRegex(acceptance.AcceptanceFailure, "Recorded event order"):
            acceptance.replay_sequence(nodes, parents, ["N02", "N01"])
        nodes[0]["timestamp"] = "2026-03-13"
        with self.assertRaisesRegex(acceptance.AcceptanceFailure, "timestamps contradict"):
            acceptance.replay_sequence(nodes, parents, ["N02"])

    def test_replay_refuses_cycles_in_unlogged_prerequisites(self):
        nodes = [{"id": "N01", "also_depends_on": ["N02"]},
                 {"id": "N02", "also_depends_on": ["N01"]}]
        with self.assertRaisesRegex(acceptance.AcceptanceFailure, "cycle"):
            acceptance.replay_sequence(nodes, {"N01": None, "N02": None}, [])

    def test_git_equivalence_cannot_hide_different_identity_mappings(self):
        directory = {"format": "ara.merge/v1", "imports": [{"original": "C05", "target": "C06"}],
                     "timings": {"commit_ms": 1}, "source_revision": "sha256:source", "conflicts": []}
        git = {**directory, "git": {"head": "commit"}, "timings": {"commit_ms": 2},
               "git_timings": {"total_setup_ms": 3}}
        self.assertEqual(acceptance.comparable_merge_report(directory), acceptance.comparable_merge_report(git))
        git["imports"] = [{"original": "C05", "target": "C07"}]
        self.assertNotEqual(acceptance.comparable_merge_report(directory), acceptance.comparable_merge_report(git))

    def test_git_journal_equivalence_keeps_opaque_clock_named_source_fields(self):
        journal = {"format": "ara.merge-log/v1", "records": [
            {"kind": "enrollment", "time": "2026-10-01T10:00", "source_key": "source", "label": "fork"},
            {"kind": "revision", "time": "2026-10-01T10:00", "git": None, "predecessor": None,
             "files": {"source.json": "e30="}, "mappings": [{"original": "N05", "target": "N06"}]},
            {"kind": "conflict", "conflict": {"field": "time", "ours": {"time": "caller value", "git": "caller extension"}}}]}
        git = json.loads(json.dumps(journal))
        git["records"][0]["time"] = "2026-10-01T11:00"
        git["records"][1]["time"] = "2026-10-01T11:00"
        git["records"][1]["git"] = {"head": "commit"}
        self.assertEqual(acceptance.comparable_merge_journal(journal), acceptance.comparable_merge_journal(git))
        git["records"][2]["conflict"]["ours"]["time"] = "different source content"
        self.assertNotEqual(acceptance.comparable_merge_journal(journal), acceptance.comparable_merge_journal(git))


class SearchFreezeTests(unittest.TestCase):
    def runner(self, root, approval=None):
        args = acceptance.argparse.Namespace(
            fixture=root / "fixture", search_approval=approval, sections=["search"], repeats=3,
            runner="isolated preflight test", toolchain="not a release measurement",
            binary=Path(sys.executable), timeout=2)
        # The real Python executable is a canary, not a simulated ara response.
        # Every scenario below must block before it can be invoked at all.
        return acceptance.Runner(args, root / "workspace")

    def labels_and_criteria(self, root):
        directory = root / "fixture/search"
        directory.mkdir(parents=True)
        relevance = {"corpus_revision": acceptance.PIN, "annotation_author": "fixture author",
                     "development": [{"query": "development control", "relevant_ids": ["N01"]}],
                     "heldout": [{"query": "heldout control", "relevant_ids": ["N02"]}]}
        pairs = {"corpus_revision": acceptance.PIN, "annotation_author": "fixture author",
                 "development": [{"left": "N01", "right": {"derived_from": "N01"}, "label": "duplicate"}],
                 "heldout": [{"left": "N02", "right": {"derived_from": "N02"}, "label": "duplicate"}]}
        for name, value in (("relevance.json", relevance), ("pairs.json", pairs)):
            (directory / name).write_text(json.dumps(value))
        criteria = {"status": "approved", "reviewer": "independent fixture reviewer",
                    "corpus_revision": acceptance.PIN, "allows_crafted_controls": True,
                    "relevance_sha256": acceptance.sha((directory / "relevance.json").read_bytes()),
                    "pairs_sha256": acceptance.sha((directory / "pairs.json").read_bytes()),
                    "development": {"recall_at_10": .9, "duplicate_precision": .9, "duplicate_recall": .8},
                    "heldout": {"recall_at_10": .8, "duplicate_precision": .9, "duplicate_recall": .8}}
        return directory, criteria

    def assert_blocked_before_invocation(self, runner):
        result = runner.search()
        self.assertEqual(result["measurement_status"], "blocked/unmeasured")
        self.assertIsNone(result["relevance"])
        self.assertIsNone(result["pairs"])
        self.assertEqual(runner.evidence["invocations"], [])
        self.assertEqual(runner.evidence["search_measurements"]["measurement_status"], "blocked/unmeasured")
        self.assertIsNone(runner.evidence["search_measurements"]["relevance"])
        self.assertIsNone(runner.evidence["search_measurements"]["pairs"])

    def test_missing_approval_blocks_without_even_requiring_label_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assert_blocked_before_invocation(self.runner(Path(temporary)))

    def test_rejected_stale_and_non_independent_decisions_never_invoke_ranker(self):
        for field, value in (("status", "rejected"), ("corpus_revision", "different revision"),
                             ("reviewer", ""), ("reviewer", "fixture author")):
            with self.subTest(field=field, value=value), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                _, criteria = self.labels_and_criteria(root)
                criteria[field] = value
                approval = root / "approval.json"
                approval.write_text(json.dumps(criteria))
                self.assert_blocked_before_invocation(self.runner(root, approval))

    def test_changed_heldout_label_bytes_block_before_any_development_or_pair_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory, criteria = self.labels_and_criteria(root)
            approval = root / "approval.json"
            approval.write_text(json.dumps(criteria))
            path = directory / "relevance.json"
            labels = json.loads(path.read_text())
            labels["heldout"][0]["query"] = "modified after criteria freeze"
            path.write_text(json.dumps(labels))
            self.assert_blocked_before_invocation(self.runner(root, approval))

    def test_invalid_frozen_thresholds_and_unapproved_crafted_controls_do_not_measure(self):
        for value in (None, True, -.1, 1.1, float("nan"), float("inf"), "0.8"):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                _, criteria = self.labels_and_criteria(root)
                criteria["heldout"]["duplicate_recall"] = value
                approval = root / "approval.json"
                approval.write_text(json.dumps(criteria))
                self.assert_blocked_before_invocation(self.runner(root, approval))
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _, criteria = self.labels_and_criteria(root)
            criteria["allows_crafted_controls"] = False
            approval = root / "approval.json"
            approval.write_text(json.dumps(criteria))
            self.assert_blocked_before_invocation(self.runner(root, approval))


if __name__ == "__main__":
    unittest.main()
