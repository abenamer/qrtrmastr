"""Unit tests for the documentation checker (adapted from the tests of the REAS checker).

Each gate of the checker has a planted violation here: a test writes a small bundle that is
valid, adds one violation, and asserts that the checker names it (REAS-DOC-006). The fixture
without a violation passes, so a failure comes from the planted line and from nothing else.
"""
from datetime import date
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from scripts.check_docs import (  # noqa: E402
    Counts,
    check,
    freshness_warnings,
    parse_document,
    retired_terms,
    split_frontmatter,
    validate,
)

TODAY = date(2026, 11, 21)
INDEX = "---\nokf_version: '0.2'\n---\n# Index\n\n[Concept](/concept.md) [Terms](/terminology.md) [Log](/log.md)\n\n[Retired terms](/terminology.md#retired-terms)\n"
CONCEPT = (
    "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Concept\n\n## Part one\n\n"
    "See [the terms](/terminology.md#retired-terms).\n"
)
TERMS = (
    "---\ntype: Technical Reference\nupdated: 2026-11-01\n---\n# Terms\n\n"
    "## Retired terms\n\n| Retired term | Replacement |\n| --- | --- |\n| oldword | new word |\n"
)
LOG = "# Log\n\n## 2026-11-01\n* **New**: a line.\n"


class FixtureTests(unittest.TestCase):
    def fixture(self, files=None):
        """A valid bundle of one concept, the terminology, a log and one vendored rule, plus `files`."""
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        base = {
            "docs/index.md": INDEX,
            "docs/concept.md": CONCEPT,
            "docs/terminology.md": TERMS,
            "docs/log.md": LOG,
            "standards/standard.md": "## REAS-TEST-001: a rule\n",
        }
        base.update(files or {})
        for name, content in base.items():
            if content is None:
                continue
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")
        return root

    def errors(self, files=None):
        return validate(self.fixture(files), TODAY)

    def run_main(self, root, *options):
        """Call the entry point of the command; return its exit status and what it printed."""
        import contextlib
        import io
        from scripts.check_docs import main

        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = main(["--root", str(root), *options])
        return status, output.getvalue()

    def assert_one_error(self, files, text):
        errors = self.errors(files)
        self.assertEqual(len(errors), 1, errors)
        self.assertIn(text, errors[0])


class FrontmatterTests(FixtureTests):
    def test_the_fixture_without_a_violation_passes(self):
        self.assertEqual(self.errors(), [])

    def test_valid_frontmatter(self):
        metadata, body, err = split_frontmatter("---\ntype: Concept\n---\nText")
        self.assertIsNone(err)
        self.assertEqual(metadata["type"], "Concept")
        self.assertEqual(body, "Text")

    def test_invalid_yaml(self):
        _, _, err = split_frontmatter("---\ntype: [unclosed\n---\nBody")
        self.assertIsNotNone(err)

    def test_duplicate_yaml_keys_are_rejected(self):
        _, _, err = split_frontmatter("---\ntype: A\ntype: B\n---\nBody")
        self.assertIn("Duplicate YAML key", err)

    def test_frontmatter_must_be_a_mapping(self):
        _, _, err = split_frontmatter("---\n- a\n---\nBody")
        self.assertEqual(err, "Frontmatter must be a mapping")

    def test_unclosed_frontmatter(self):
        _, _, err = split_frontmatter("---\ntype: A\nBody")
        self.assertEqual(err, "Unclosed YAML frontmatter")

    def test_a_concept_without_a_type_is_rejected(self):
        self.assert_one_error(
            {"docs/concept.md": "---\ntitle: no type\nupdated: 2026-11-01\n---\nBody\n"},
            "docs/concept.md: Concept requires a non-empty type",
        )

    def test_a_concept_without_frontmatter_is_rejected(self):
        self.assert_one_error({"docs/concept.md": "# Concept\n"}, "requires a non-empty type")

    def test_an_empty_concept_is_rejected(self):
        self.assert_one_error(
            {"docs/concept.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n"}, "Empty concept"
        )

    def test_a_missing_bundle_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            self.assertEqual(validate(Path(temporary), TODAY), ["The docs/ OKF bundle is missing"])

    def test_a_missing_index_is_rejected(self):
        errors = self.errors({"docs/index.md": None})
        self.assertTrue(any("Bundle index is missing" in error for error in errors), errors)

    def test_a_wrong_index_version_is_rejected(self):
        self.assert_one_error(
            {"docs/index.md": INDEX.replace("0.2", "0.1")}, "Bundle index must contain only okf_version"
        )

    def test_nested_index_frontmatter_is_rejected(self):
        self.assert_one_error(
            {"docs/sub/index.md": "---\ntype: Concept\n---\n# Sub\n"},
            "Nested index must not contain frontmatter",
        )

    def test_the_log_takes_no_frontmatter_and_valid_dates(self):
        self.assert_one_error({"docs/log.md": "---\ntype: Log\n---\n" + LOG}, "Log must not contain frontmatter")
        self.assert_one_error(
            {"docs/log.md": "# Log\n\n## 2026-13-01\n* a line\n"}, "Log date must use YYYY-MM-DD: 2026-13-01"
        )
        self.assert_one_error({"docs/log.md": "# Log\n\n## Yesterday\n"}, "Log date must use YYYY-MM-DD")

    def test_event_metadata_needs_an_actor_and_a_time_with_an_offset(self):
        good = CONCEPT.replace(
            "updated:", 'generated: { by: "process:tool", at: "2026-11-01T10:00:00Z" }\nupdated:'
        )
        self.assertEqual(self.errors({"docs/concept.md": good}), [])
        self.assert_one_error(
            {"docs/concept.md": good.replace("process:tool", "somebody")}, "Invalid generated.by"
        )
        self.assert_one_error(
            {"docs/concept.md": good.replace("T10:00:00Z", "T10:00:00")}, "Invalid generated.at"
        )
        self.assert_one_error(
            {"docs/concept.md": good.replace('"2026-11-01T10:00:00Z"', "2026-11-01")}, "Invalid generated.at"
        )

    def test_a_source_needs_a_resource(self):
        self.assert_one_error(
            {"docs/concept.md": CONCEPT.replace("updated:", "sources:\n  - id: x\nupdated:")},
            "Sources must include a resource",
        )


class LinkTests(FixtureTests):
    def test_a_broken_link_is_rejected_in_each_form(self):
        for line in (
            "[missing](absent.md)",
            "[missing][ref]\n\n[ref]: absent.md",
            "![image](absent.png)",
            '<a href="absent.md">missing</a>',
        ):
            self.assert_one_error({"docs/concept.md": CONCEPT + "\n" + line + "\n"}, "Broken local link")

    def test_code_declares_no_link_and_no_rule(self):
        body = CONCEPT + "\n```md\n[missing](absent.md)\n## REAS-TEST-001\n```\n\n`[missing](absent.md)`\n"
        self.assertEqual(self.errors({"docs/concept.md": body}), [])

    def test_a_broken_fragment_is_rejected(self):
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\n[part](#part-two)\n"}, "Broken local fragment: #part-two"
        )
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\n[terms](/terminology.md#no-such-heading)\n"},
            "Broken local fragment",
        )

    def test_heading_slugs_follow_the_github_form(self):
        document = parse_document("# A `code` Title, with: marks!\n\n## Twice\n\n## Twice\n\n<a id=\"fixed\"></a>\n")
        self.assertEqual(document.anchors, {"a-code-title-with-marks", "twice", "twice-1", "fixed"})
        self.assertEqual(
            self.errors({"docs/concept.md": CONCEPT + "\n[one](#part-one) [top](#concept)\n"}), []
        )

    def test_an_absolute_path_is_read_from_the_bundle_inside_it_and_from_the_root_outside_it(self):
        self.assertEqual(self.errors({"README.md": "[the concept](docs/concept.md) [same](/docs/concept.md)\n"}), [])
        self.assert_one_error({"README.md": "[the concept](/concept.md)\n"}, "README.md: Broken local link")
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\n[wrong](/docs/concept.md)\n"}, "Broken local link"
        )

    def test_a_link_cannot_leave_the_repository(self):
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\n[out](../../outside.md)\n"}, "Link leaves repository"
        )

    def test_an_external_link_is_not_fetched(self):
        body = CONCEPT + "\n[site](https://example.invalid/x#y) <mailto:a@example.invalid>\n"
        self.assertEqual(self.errors({"docs/concept.md": body}), [])

    def test_an_invalid_uri_is_an_error_and_not_a_crash(self):
        errors = self.errors({"docs/concept.md": CONCEPT + '\n<a href="http://[bad">x</a>\n'})
        self.assertEqual(len(errors), 1, errors)
        self.assertIn("Invalid link URI", errors[0])


class NavigationTests(FixtureTests):
    def test_a_concept_that_the_index_does_not_reach_is_rejected(self):
        self.assert_one_error(
            {"docs/alone.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Alone\n"},
            "docs/alone.md: Concept is not reachable from docs/index.md",
        )

    def test_a_concept_reached_through_another_concept_is_accepted(self):
        files = {
            "docs/concept.md": CONCEPT + "\n[second](/second.md)\n",
            "docs/second.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Second\n",
        }
        self.assertEqual(self.errors(files), [])

    def test_a_path_through_a_file_outside_the_bundle_does_not_count(self):
        files = {
            "docs/concept.md": CONCEPT + "\n[readme](../README.md)\n",
            "README.md": "[alone](docs/alone.md)\n",
            "docs/alone.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Alone\n",
        }
        self.assert_one_error(files, "docs/alone.md: Concept is not reachable")


class RuleTests(FixtureTests):
    def test_a_rule_declared_twice_is_rejected(self):
        files = {
            "standards/a.md": "## REAS-TEST-002: one\n",
            "standards/b.md": "## REAS-TEST-002: again\n",
        }
        self.assert_one_error(files, "Duplicate REAS-TEST-002")

    def test_a_tree_without_a_rule_declaration_fails_on_the_floor(self):
        self.assert_one_error({"standards/standard.md": None}, "Floor: no rule declaration was found")

    def test_a_malformed_rule_identifier_is_rejected(self):
        self.assert_one_error({"standards/a.md": "## REAS-test-1: bad\n"}, "Invalid rule declaration")


class UpdatedTests(FixtureTests):
    def concept(self, updated, kind="Concept"):
        return {"docs/concept.md": CONCEPT.replace("type: Concept", f"type: {kind}").replace("2026-11-01", updated)}

    def test_a_concept_without_an_updated_date_is_rejected(self):
        self.assert_one_error(
            {"docs/concept.md": CONCEPT.replace("updated: 2026-11-01\n", "")},
            "docs/concept.md: Concept requires an updated date",
        )

    def test_an_invalid_date_and_a_date_after_today_are_rejected(self):
        self.assert_one_error(self.concept("'2026-02-30'"), "Invalid updated")
        self.assert_one_error(self.concept("20261101"), "Invalid updated")
        self.assert_one_error(self.concept("last week"), "Invalid updated")
        self.assert_one_error(self.concept("2026-11-22"), "is in the future")
        self.assertEqual(self.errors(self.concept("2026-11-21")), [])

    def test_a_date_that_no_calendar_holds_is_an_error_and_not_a_crash(self):
        # YAML reads an unquoted date itself and refuses the thirtieth of February.
        errors = self.errors(self.concept("2026-02-30"))
        self.assertTrue(any("docs/concept.md: Invalid YAML" in error for error in errors), errors)

    def test_a_stale_concept_is_a_warning_and_not_an_error(self):
        root = self.fixture(self.concept("2025-11-20"))
        self.assertEqual(validate(root, TODAY), [])
        warnings = freshness_warnings(root, TODAY)
        self.assertEqual(len(warnings), 1, warnings)
        self.assertIn("docs/concept.md: updated 2025-11-20 is 366 days old; the limit for Concept is 365", warnings[0])

    def test_the_threshold_depends_on_the_type(self):
        for kind, limit in (("Audit Report", 90), ("Technical Reference", 180), ("Review Procedure", 180), ("Runbook", 365)):
            at_limit = date.fromordinal(TODAY.toordinal() - limit).isoformat()
            past_limit = date.fromordinal(TODAY.toordinal() - limit - 1).isoformat()
            self.assertEqual(freshness_warnings(self.fixture(self.concept(at_limit, kind)), TODAY), [], kind)
            self.assertEqual(len(freshness_warnings(self.fixture(self.concept(past_limit, kind)), TODAY)), 1, kind)


class RetiredTermTests(FixtureTests):
    def test_a_planted_retired_term_is_rejected(self):
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\nThis is the OldWord of the text.\n"},
            "docs/concept.md: Retired term 'oldword'; use new word",
        )
        self.assert_one_error({"README.md": "An oldword outside the bundle.\n"}, "README.md: Retired term")
        self.assert_one_error(
            {"docs/concept.md": CONCEPT + "\n| a | oldword |\n| --- | --- |\n"}, "Retired term"
        )

    def test_a_retired_term_in_code_is_not_flagged(self):
        body = CONCEPT + "\nThe flag `oldword` stays.\n\n```\noldword\n```\n"
        self.assertEqual(self.errors({"docs/concept.md": body}), [])

    def test_the_exempt_documents_can_name_a_retired_term(self):
        files = {
            "docs/log.md": LOG + "* **Update**: oldword became new word.\n",
            "CHANGELOG.md": "- oldword became new word.\n",
            "docs/ste-review.md": "---\ntype: Audit Report\nupdated: 2026-11-01\n---\n# Review\n\noldword\n",
            "docs/index.md": INDEX + "[Review](/ste-review.md)\n",
            "standards/quoted.md": "The standard says oldword.\n",
        }
        self.assertEqual(self.errors(files), [])

    def test_a_missing_or_empty_table_fails(self):
        without = TERMS.split("## Retired terms")[0]
        errors = self.errors({"docs/terminology.md": without, "docs/concept.md": CONCEPT.replace("#retired-terms", "")})
        self.assertIn("docs/terminology.md: Retired terms section is missing", errors)
        empty = TERMS.replace("| oldword | new word |\n", "")
        self.assertIn("docs/terminology.md: Retired terms table is empty", self.errors({"docs/terminology.md": empty}))

    def test_a_bundle_without_the_terminology_fails(self):
        errors = self.errors({"docs/terminology.md": None, "docs/index.md": "---\nokf_version: '0.2'\n---\n# Index\n\n[Concept](/concept.md) [Log](/log.md)\n",
                              "docs/concept.md": CONCEPT.replace("(/terminology.md#retired-terms)", "(#part-one)")})
        self.assertIn("docs/terminology.md: The terminology concept is missing", errors)

    def test_the_parser_reads_the_term_and_its_replacement(self):
        self.assertEqual(retired_terms(TERMS), [("oldword", "new word")])
        self.assertIsNone(retired_terms("# Terms\n"))


class FloorTests(FixtureTests):
    def test_a_gate_that_looked_at_nothing_fails(self):
        self.assertEqual(Counts(concepts=1, links=1, fragments=1, retired_terms=1, documents_read_for_terms=1, rules=1).floor_errors(), [])
        self.assertEqual(len(Counts().floor_errors()), 5)
        for name in ("concepts", "links", "fragments", "retired_terms", "documents_read_for_terms", "rules"):
            counts = Counts(concepts=1, links=1, fragments=1, retired_terms=1, documents_read_for_terms=1, rules=1)
            setattr(counts, name, 0)
            self.assertEqual(len(counts.floor_errors()), 1, name)

    def test_a_bundle_with_no_link_and_no_fragment_fails_on_the_floors(self):
        files = {
            "docs/index.md": "---\nokf_version: '0.2'\n---\n# Index\n",
            "docs/concept.md": None,
            "docs/log.md": None,
        }
        errors = self.errors(files)
        self.assertIn("Floor: no local link was checked", errors)
        self.assertIn("Floor: no local fragment was checked", errors)

    def test_the_counts_of_the_fixture_are_the_ones_it_holds(self):
        counts = check(self.fixture(), TODAY).counts
        self.assertEqual(
            (counts.concepts, counts.links, counts.fragments, counts.retired_terms, counts.documents_read_for_terms, counts.rules),
            (2, 5, 2, 1, 2, 1),
        )


class FileTests(FixtureTests):
    def test_a_markdown_file_with_another_extension_is_rejected(self):
        self.assert_one_error({"docs/page.MD": CONCEPT}, "docs/page.MD: A Markdown file must have the extension .md")
        self.assert_one_error({"notes.markdown": "text\n"}, "notes.markdown: A Markdown file must have the extension .md")

    def test_a_symbolic_link_is_rejected_and_its_target_is_not_read(self):
        root = self.fixture()
        with tempfile.TemporaryDirectory() as outside:
            secret = Path(outside) / "secret.md"
            secret.write_text("---\ntype: [unclosed\n---\n# REAS-bad heading with SECRET-TEXT\n\n[x](SECRET-LINK.md)\n")
            (root / "docs" / "leak.md").symlink_to(secret)
            errors = validate(root, TODAY)
        self.assertEqual(errors, ["docs/leak.md: A Markdown file must not be a symbolic link"])

    def test_a_directory_is_skipped_only_at_the_top_of_the_repository(self):
        self.assertEqual(self.errors({"target/doc/page.md": "[x](absent.md)\n", ".venv/page.md": "oldword\n"}), [])
        self.assert_one_error({"docs/target/page.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Page\n"}, "docs/target/page.md: Concept is not reachable")

    def test_a_file_named_log_below_the_top_of_the_bundle_is_a_concept(self):
        errors = self.errors({"docs/sub/log.md": "# A page\n\nText.\n"})
        self.assertIn("docs/sub/log.md: Concept requires a non-empty type", errors)
        self.assertIn("docs/sub/log.md: Concept is not reachable from docs/index.md", errors)

    def test_a_file_that_is_not_utf8_is_an_error(self):
        root = self.fixture()
        (root / "docs" / "bytes.md").write_bytes(b"\xff\xfe bad")
        self.assertTrue(any("docs/bytes.md: Cannot read" in error for error in validate(root, TODAY)))


class CrashTests(FixtureTests):
    def test_an_input_that_the_parts_below_refuse_is_an_error_and_not_a_crash(self):
        self.assert_one_error({"docs/concept.md": CONCEPT + "\n[x](a%00b.md)\n"}, "Broken local link")
        self.assert_one_error({"docs/concept.md": CONCEPT + "\n[x](" + "n" * 300 + ".md)\n"}, "Broken local link")
        errors = self.errors({"docs/concept.md": "---\nkey: " + "[" * 2000 + "\n---\n# Concept\n"})
        self.assertTrue(any("docs/concept.md: Invalid YAML" in error for error in errors), errors)
        root = self.fixture({"docs/concept.md": CONCEPT.replace("type: Concept", "type: [a, b]").replace("2026-11-01", "2020-01-01")})
        self.assertIn("docs/concept.md: Concept requires a non-empty type", validate(root, TODAY))
        self.assertEqual(len(freshness_warnings(root, TODAY)), 1)

    def test_a_link_to_a_file_with_a_slash_after_its_name_is_broken(self):
        for link in ("/terminology.md/", "terminology.md/", "terminology.md/.", "terminology.md/x/..", "/terminology.md//"):
            self.assert_one_error({"docs/concept.md": CONCEPT + f"\n[x]({link})\n"}, f"Broken local link: {link}")
        self.assertEqual(self.errors({"docs/concept.md": CONCEPT + "\n[x](sub/../terminology.md) [y](./terminology.md)\n", "docs/sub/index.md": "# Sub\n"}), [])
        self.assertEqual(self.errors({"docs/concept.md": CONCEPT + "\n[x](/sub/)\n", "docs/sub/index.md": "# Sub\n"}), [])


class VerifiedEventTests(FixtureTests):
    def with_verified(self, value):
        return {"docs/concept.md": CONCEPT.replace("updated:", f"verified: {value}\nupdated:")}

    def test_a_verified_event_is_a_mapping_or_a_list_of_mappings_with_actor_and_time(self):
        good = '{ by: "human:reviewer", at: "2026-11-01T10:00:00+01:00" }'
        self.assertEqual(self.errors(self.with_verified(good)), [])
        self.assertEqual(self.errors(self.with_verified(f"[{good}, {good}]")), [])
        self.assert_one_error(self.with_verified("yes"), "verified must be a mapping or a list")
        self.assert_one_error(self.with_verified("[yes]"), "verified must be a mapping")
        self.assert_one_error(self.with_verified(good.replace("human:reviewer", "a reviewer")), "Invalid verified.by")
        self.assert_one_error(self.with_verified(good.replace("+01:00", "")), "Invalid verified.at")
        self.assert_one_error(self.concept_generated("today"), "generated must be a mapping")

    def concept_generated(self, value):
        return {"docs/concept.md": CONCEPT.replace("updated:", f"generated: {value}\nupdated:")}


class UpdatedFormTests(FixtureTests):
    def test_only_a_day_is_a_date(self):
        for value in ("'20261101'", "'2026-W44-7'", "2026-11-01T10:00:00Z", "2026-11-01 10:00:00", "[2026-11-01]", "null"):
            errors = self.errors({"docs/concept.md": CONCEPT.replace("updated: 2026-11-01", f"updated: {value}")})
            self.assertEqual(len(errors), 1, (value, errors))
            self.assertIn("Invalid updated", errors[0])
        self.assertEqual(self.errors({"docs/concept.md": CONCEPT.replace("updated: 2026-11-01", "updated: '2026-11-01'")}), [])


class RetiredTermPlaceTests(FixtureTests):
    def test_a_retired_term_is_found_in_each_place_that_a_reader_sees(self):
        for planted in (
            CONCEPT + "\n<p>The oldword list.</p>\n",
            CONCEPT + '\nText with <span title="x">oldword</span> in a tag.\n',
            CONCEPT + '\n[link](/terminology.md "the oldword")\n',
            CONCEPT.replace("type: Concept", "type: Concept\ntitle: The oldword page"),
            CONCEPT.replace("type: Concept", "type: Concept\ndescription: About the oldword."),
            CONCEPT + "\n## The oldword heading\n",
            CONCEPT + "\n[the oldword](/terminology.md)\n",
            CONCEPT + "\n![the oldword](/terminology.md)\n",
            CONCEPT + "\nAn **oldword** in bold.\n",
        ):
            self.assert_one_error({"docs/concept.md": planted}, "docs/concept.md: Retired term 'oldword'")

    def test_the_end_of_a_line_stands_between_two_words(self):
        terms = TERMS.replace("| oldword | new word |", "| olden word | new word |")
        self.assert_one_error(
            {"docs/terminology.md": terms, "docs/concept.md": CONCEPT + "\nThis is the olden\nword in two lines.\n"},
            "Retired term 'olden word'",
        )
        # Two words that only touch at the end of a line are not the term.
        self.assertEqual(self.errors({"docs/concept.md": CONCEPT + "\nIt was cold\nwords came later.\n"}), [])

    def test_a_retired_term_with_markup_in_the_table_is_rejected(self):
        for cell in ("`oldword`", "*oldword*", "[oldword](x.md)"):
            errors = self.errors({"docs/terminology.md": TERMS.replace("| oldword |", f"| {cell} |")})
            self.assertIn("docs/terminology.md: A retired term must be plain text, with no markup", errors, cell)


class CommandTests(FixtureTests):
    def test_the_exit_status_is_zero_only_when_no_gate_failed(self):
        status, output = self.run_main(self.fixture())
        self.assertEqual(status, 0)
        self.assertIn("PASS:", output)
        self.assertIn("CHECKED: 2 concepts, 5 local links, 2 fragments, 1 retired terms in 2 documents, 1 rule declarations", output)
        status, output = self.run_main(self.fixture({"docs/concept.md": CONCEPT + "\n[x](absent.md)\n"}))
        self.assertEqual(status, 1)
        self.assertIn("ERROR: docs/concept.md: Broken local link: absent.md", output)
        self.assertIn("FAIL: 1 errors", output)
        self.assertNotIn("PASS", output)

    def test_strict_freshness_makes_a_stale_concept_an_error(self):
        root = self.fixture({"docs/concept.md": CONCEPT.replace("2026-11-01", "2020-01-01")})
        status, output = self.run_main(root)
        self.assertEqual(status, 0)
        self.assertIn("WARN: docs/concept.md: updated 2020-01-01", output)
        status, output = self.run_main(root, "--strict-freshness")
        self.assertEqual(status, 1)
        self.assertIn("ERROR: docs/concept.md: updated 2020-01-01", output)

    def test_a_control_character_of_a_document_is_printed_as_an_escape(self):
        hostile = CONCEPT + '\n<a href="./absent\n::error file=README.md::forged\x1b[2J">x</a>\n'
        status, output = self.run_main(self.fixture({"docs/concept.md": hostile}))
        self.assertEqual(status, 1)
        self.assertIn("Broken local link: ./absent\\u000a::error file=README.md::forged\\u001b[2J", output)
        self.assertFalse(any(line.startswith("::") for line in output.splitlines()), output)
        self.assertNotIn("\x1b", output)


class SecondPassTests(FixtureTests):
    def test_the_older_command_form_of_a_runner_is_broken_up_in_each_message(self):
        files = {
            "docs/concept.md": CONCEPT.replace("type: Concept", "type: 'A##[error]kind'").replace("2026-11-01", "2020-01-01")
            + '\n<a href="absent##[error]x.md">x</a>\n',
        }
        status, output = self.run_main(self.fixture(files))
        self.assertEqual(status, 1)
        self.assertIn("WARN:", output)
        self.assertIn("Broken local link: absent#\\u0023[error]x.md", output)
        self.assertNotIn("##[", output)

    def test_a_warning_is_printed_with_its_control_characters_as_escapes(self):
        files = {"docs/concept.md": CONCEPT.replace("type: Concept", 'type: "A\\nkind\\e"').replace("2026-11-01", "2020-01-01")}
        status, output = self.run_main(self.fixture(files))
        self.assertEqual(status, 0)
        self.assertIn("the limit for A\\u000akind\\u001b is 365", output)

    def test_html_that_the_parser_refuses_is_an_error_and_not_a_crash(self):
        from scripts.check_docs import HTMLReferences

        class Refusing(HTMLReferences):
            def feed(self, text):
                raise AssertionError("we should not get here!")

        reader = Refusing()
        reader.read("<div>")
        self.assertEqual(reader.unreadable, 1)
        # Whatever the Python version does with this block, the checker gives a list of errors.
        errors = self.errors({"docs/concept.md": CONCEPT + "\n<div>\n<![foo[ x ]]>\n</div>\n"})
        self.assertTrue(all("Cannot read a piece of HTML" in error for error in errors), errors)

    def test_a_stale_page_behind_a_symbolic_link_gives_no_warning(self):
        root = self.fixture()
        with tempfile.TemporaryDirectory() as outside:
            page = Path(outside) / "old.md"
            page.write_text(CONCEPT.replace("2026-11-01", "2020-01-01"))
            (root / "docs" / "old.md").symlink_to(page)
            self.assertEqual(freshness_warnings(root, TODAY), [])

    def test_more_places_and_forms_of_a_retired_term(self):
        for planted in (
            CONCEPT + '\nA <span title="oldword">word</span> in a tag.\n',
            CONCEPT + '\n![image](/terminology.md "the oldword")\n',
        ):
            self.assert_one_error({"docs/concept.md": planted}, "Retired term 'oldword'")
        terms = TERMS.replace("| oldword | new word |", "| olden word | new word |")
        self.assert_one_error(
            {"docs/terminology.md": terms, "docs/concept.md": CONCEPT + "\nThis is the olden  \nword after a hard break.\n"},
            "Retired term 'olden word'",
        )
        for cell in ("_oldword_", "<b>oldword</b>"):
            errors = self.errors({"docs/terminology.md": TERMS.replace("| oldword |", f"| {cell} |")})
            self.assertIn("docs/terminology.md: A retired term must be plain text, with no markup", errors, cell)


class RealRepositoryTests(unittest.TestCase):
    def test_the_repository_passes(self):
        self.assertEqual(validate(ROOT), [])

    def test_the_repository_gives_each_gate_something_to_read(self):
        counts = check(ROOT).counts
        self.assertGreaterEqual(counts.concepts, 29)
        self.assertGreaterEqual(counts.links, 300)
        self.assertGreaterEqual(counts.fragments, 50)
        self.assertGreaterEqual(counts.retired_terms, 2)
        self.assertGreaterEqual(counts.documents_read_for_terms, 30)
        # The rules are declared in the vendored copy of the standard, 44 in 1.7.0-draft.
        self.assertGreaterEqual(counts.rules, 44)


if __name__ == "__main__":
    unittest.main()
