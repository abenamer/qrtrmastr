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
    SKIPPED_DIRECTORIES,
    Counts,
    authority_errors,
    authority_rows,
    check,
    committed_files,
    freshness_warnings,
    parse_document,
    retired_terms,
    split_frontmatter,
    validate,
)

TODAY = date(2026, 11, 21)
INDEX = "---\nokf_version: '0.2'\n---\n# Index\n\n[Concept](/concept.md) [Terms](/terminology.md) [Log](/log.md) [Files](/reference/files.md)\n\n[Retired terms](/terminology.md#retired-terms)\n"
CONCEPT = (
    "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Concept\n\n## Part one\n\n"
    "See [the terms](/terminology.md#retired-terms).\n"
)
TERMS = (
    "---\ntype: Technical Reference\nupdated: 2026-11-01\n---\n# Terms\n\n"
    "## Retired terms\n\n| Retired term | Replacement |\n| --- | --- |\n| oldword | new word |\n"
)
LOG = "# Log\n\n## 2026-11-01\n* **New**: a line.\n"
FILES_HEAD = (
    "---\ntype: Technical Reference\nupdated: 2026-11-01\n---\n# Files\n\n"
    "## The class of each file\n\n| Path | Class | Who or what writes it |\n| --- | --- | --- |\n"
)
# One row for each file of the fixture, for the tests of the gate itself.
FILES_ROWS = (
    "| `docs/log.md` | Evidence | The record of each change. |\n"
    "| `docs/` | Authored | The bundle. |\n"
    "| `standards/` | Derived, committed | A copy; `standards/README.md` names the commit. |\n"
    "| `LICENSE` | Authored | The license. |\n"
)
FILES = FILES_HEAD + FILES_ROWS
FILES_PATH = "docs/reference/files.md"


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
            "LICENSE": "The license of the fixture.\n",
        }
        base.update(files or {})
        if FILES_PATH not in base:
            # A test of another gate adds and removes files. Its page of the authority classes
            # has one row for each file that the fixture then holds, so that gate has nothing
            # to say; a test of the authority classes brings its own page.
            names = sorted(
                name for name, content in base.items()
                if content is not None and name.split("/")[0] not in SKIPPED_DIRECTORIES
            )
            base[FILES_PATH] = FILES_HEAD + "".join(
                f"| `{name}` | Authored | A file of the fixture. |\n" for name in [FILES_PATH] + names
            )
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
        self.assertEqual(Counts(concepts=1, links=1, fragments=1, retired_terms=1, documents_read_for_terms=1, rules=1, files_with_a_class=1).floor_errors(), [])
        self.assertEqual(len(Counts().floor_errors()), 6)
        for name in ("concepts", "links", "fragments", "retired_terms", "documents_read_for_terms", "rules", "files_with_a_class"):
            counts = Counts(concepts=1, links=1, fragments=1, retired_terms=1, documents_read_for_terms=1, rules=1, files_with_a_class=1)
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
            (counts.concepts, counts.links, counts.fragments, counts.retired_terms, counts.documents_read_for_terms, counts.rules, counts.files_with_a_class),
            (3, 6, 2, 1, 3, 1, 7),
        )


class FileTests(FixtureTests):
    def test_a_markdown_file_with_another_extension_is_rejected(self):
        self.assert_one_error({"docs/page.MD": CONCEPT}, "docs/page.MD: A Markdown file must have the extension .md")
        self.assert_one_error({"notes.markdown": "text\n"}, "notes.markdown: A Markdown file must have the extension .md")

    def test_a_symbolic_link_is_rejected_and_its_target_is_not_read(self):
        # The page with a row for the directory: the link is made after the fixture is written.
        root = self.fixture({FILES_PATH: FILES})
        with tempfile.TemporaryDirectory() as outside:
            secret = Path(outside) / "secret.md"
            secret.write_text("---\ntype: [unclosed\n---\n# REAS-bad heading with SECRET-TEXT\n\n[x](SECRET-LINK.md)\n")
            (root / "docs" / "leak.md").symlink_to(secret)
            errors = validate(root, TODAY)
        self.assertEqual(errors, ["docs/leak.md: A Markdown file must not be a symbolic link"])

    def test_a_directory_is_skipped_only_at_the_top_of_the_repository(self):
        self.assertEqual(self.errors({"target/doc/page.md": "[x](absent.md)\n", ".venv/page.md": "oldword\n"}), [])
        self.assert_one_error({"docs/target/page.md": "---\ntype: Concept\nupdated: 2026-11-01\n---\n# Page\n"}, "docs/target/page.md: Concept is not reachable")

    def test_the_build_of_the_fuzz_package_is_skipped_and_its_sources_are_not(self):
        # The page of a library in the build directory is no document of the project.
        self.assertEqual(self.errors({"fuzz/target/release/build/lib/out/README.md": "[x](absent.md) oldword\n"}), [])
        self.assertEqual(self.errors({"fuzz/corpus/mime/found.md": "[x](absent.md)\n"}), [])
        # The control: a page beside the targets is read.
        errors = self.errors({"fuzz/README.md": "[x](absent.md)\n"})
        self.assertIn("fuzz/README.md: Broken local link: absent.md", errors)

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
        self.assertIn("CHECKED: 3 concepts, 6 local links, 2 fragments, 1 retired terms in 3 documents, 1 rule declarations, 7 files with an authority class", output)
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


class AuthorityClassTests(FixtureTests):
    """REAS-DOC-002: each committed file has one authority class, from the table of one page."""

    def files_page(self, rows):
        return {"docs/reference/files.md": FILES_HEAD + rows}

    def test_the_fixture_gives_each_of_its_files_a_class(self):
        for files in (None, {FILES_PATH: FILES}):
            result = check(self.fixture(files), TODAY)
            self.assertEqual(result.errors, [])
            # Index, concept, terminology, log, the page itself, the vendored rule, the license.
            self.assertEqual(result.counts.files_with_a_class, 7)

    def test_a_file_that_no_row_matches_has_no_class(self):
        files = self.files_page(FILES_ROWS)
        files["scripts/new.sh"] = "echo\n"
        self.assertEqual(self.errors(files), ["scripts/new.sh: No authority class; add a row to docs/reference/files.md"])
        # The control: with a row for it, the same file passes.
        files = self.files_page(FILES_ROWS + "| `scripts/` | Authored | The checks. |\n")
        files["scripts/new.sh"] = "echo\n"
        self.assertEqual(self.errors(files), [])

    def test_a_file_takes_the_first_row_that_matches_it(self):
        rows = authority_rows(FILES)[:3]
        self.assertEqual(rows[0], ("`docs/log.md`", "Evidence", "The record of each change."))
        # The row of the log stands above the row of its directory, so each row takes a file.
        errors, with_a_class = authority_errors(rows, ["docs/log.md", "docs/index.md", "standards/a.md"])
        self.assertEqual((errors, with_a_class), ([], 3))
        # Below the row of the directory, the row of the log takes no file.
        below = [rows[1], rows[0], rows[2]]
        errors, with_a_class = authority_errors(below, ["docs/log.md", "docs/index.md", "standards/a.md"])
        self.assertEqual(with_a_class, 3)
        self.assertEqual(len(errors), 1)
        self.assertIn("'docs/log.md': No file has this row as its first match", errors[0])

    def test_a_row_that_no_file_takes_fails(self):
        errors = self.errors(self.files_page(FILES_ROWS + "| `migrations/` | Authored | The schema. |\n"))
        self.assertEqual(len(errors), 1)
        self.assertIn("'migrations/': No file has this row as its first match", errors[0])

    def test_a_class_that_is_not_one_of_the_four_fails(self):
        for word in ("Generated", "authored", "Derived", ""):
            rows = FILES_ROWS.replace("| `docs/` | Authored |", f"| `docs/` | {word} |")
            errors = self.errors(self.files_page(rows))
            self.assertEqual(errors, [f"docs/reference/files.md: 'docs/': '{word}' is not an authority class"], word)

    def test_a_derived_row_names_its_command_or_record(self):
        rows = FILES_ROWS.replace("A copy; `standards/README.md` names the commit.", "A copy of the source.")
        errors = self.errors(self.files_page(rows))
        self.assertEqual(len(errors), 1)
        self.assertIn("'standards/': A derived file must name, in a code span", errors[0])

    def test_a_companion_row_names_what_it_summarizes(self):
        rows = "| `docs/concept.md` | Companion | |\n" + FILES_ROWS
        errors = self.errors(self.files_page(rows))
        self.assertEqual(errors, ["docs/reference/files.md: 'docs/concept.md': A companion file must name what it summarizes"])
        rows = "| `docs/concept.md` | Companion | A summary of the terms. |\n" + FILES_ROWS
        self.assertEqual(self.errors(self.files_page(rows)), [])

    def test_a_path_is_a_file_a_directory_or_a_name_with_a_star(self):
        rows = [("`a.md`", "Authored", ""), ("`dir/`", "Authored", ""), ("`*.lock`", "Derived, committed", "`cargo`")]
        files = ["a.md", "dir/b.md", "dir/deep/c.md", "x.lock", "dir2/y.lock"]
        self.assertEqual(authority_errors(rows, files), ([], 5))
        # A file name is the whole path, a directory needs its slash, and a prefix is no match.
        for other in ("a.md.bak", "sub/a.md", "dir", "directory/b.md", "x.locked", "A.md", "X.LOCK"):
            errors, _ = authority_errors(rows, files + [other])
            self.assertEqual(errors, [f"{other}: No authority class; add a row to docs/reference/files.md"], other)
        # A row without the slash is one file of that name, not the directory.
        errors, with_a_class = authority_errors([("`dir`", "Authored", "")], ["dir", "dir/b.md"])
        self.assertEqual((errors, with_a_class), (["dir/b.md: No authority class; add a row to docs/reference/files.md"], 1))

    def test_a_row_for_everything_and_a_pattern_with_another_wildcard_fail(self):
        for pattern in ("*", "**", "*/", "/", "file?.txt", "a[1]*", "*.[ch]"):
            errors, with_a_class = authority_errors([(f"`{pattern}`", "Authored", "")], ["file1.txt", "a1", "x.c"])
            self.assertEqual(with_a_class, 0, pattern)
            self.assertEqual(
                errors[0],
                f"docs/reference/files.md: '{pattern}': A path must name a file, a directory or a part of a name; `*` is the one wildcard",
            )
        # The control: a star with a part of a name is a pattern.
        self.assertEqual(authority_errors([("`*.c`", "Authored", "")], ["x.c"]), ([], 1))
        # In the page, a row for everything gives no file a class.
        errors = self.errors(self.files_page("| `*` | Authored | Everything. |\n"))
        self.assertIn("Floor: no file was given an authority class", errors)

    def test_a_row_in_a_comment_or_a_code_block_or_with_an_indent_is_not_read_as_a_row(self):
        hidden = "| `scripts/` | Authored | The checks. |\n"
        for section in (
            FILES_ROWS + "\n<!--\n" + hidden + "-->\n",
            FILES_ROWS + "\n```\n" + hidden + "```\n",
            FILES_ROWS + "\n~~~\n" + hidden + "~~~\n",
            FILES_ROWS + "    " + hidden,
            FILES_ROWS + " " + hidden,
        ):
            files = self.files_page(section)
            files["scripts/new.sh"] = "echo\n"
            errors = self.errors(files)
            self.assertEqual(len([e for e in errors if e.startswith("docs/reference/files.md: ")]), 1, section)
            self.assertIn("Floor: no file was given an authority class", errors, section)
        self.assertEqual(authority_rows(FILES_HEAD + FILES_ROWS + "\n<!-- note -->\n"), "The section must hold no comment and no code block")
        self.assertEqual(authority_rows(FILES_HEAD + FILES_ROWS + " | `x` | Authored | |\n"), "A line of the table must start at the margin")

    def test_the_table_starts_with_its_header_row(self):
        without_header = FILES.replace("| Path | Class | Who or what writes it |\n| --- | --- | --- |\n", "")
        self.assertNotEqual(without_header, FILES)
        self.assertEqual(authority_rows(without_header), "The table must start with the header row: Path, Class and one more column")
        errors = self.errors({FILES_PATH: without_header})
        self.assertIn("docs/reference/files.md: The table must start with the header row: Path, Class and one more column", errors)
        for header in ("| Class | Path | Who |", "| Path | Class |", "| Path | Class | Who | More |"):
            page = FILES.replace("| Path | Class | Who or what writes it |", header)
            self.assertIsInstance(authority_rows(page), str, header)

    def test_a_path_that_is_not_one_code_span_fails(self):
        for cell in ("docs/", "`docs/` and `src/`", "**`docs/`**"):
            rows = FILES_ROWS.replace("| `docs/` |", f"| {cell} |")
            errors = self.errors(self.files_page(rows))
            self.assertIn(f"docs/reference/files.md: '{cell}': The path of a row must be one code span", errors, cell)

    def test_the_page_its_section_and_its_table_must_exist(self):
        self.assertIn(
            "docs/reference/files.md: The page of the authority classes is missing",
            self.errors({"docs/reference/files.md": None, "docs/index.md": INDEX.replace(" [Files](/reference/files.md)", "")}),
        )
        no_section = FILES.replace("## The class of each file", "## Another heading")
        self.assertNotEqual(no_section, FILES)
        self.assertIn(
            "docs/reference/files.md: The section '## The class of each file' is missing",
            self.errors({"docs/reference/files.md": no_section}),
        )
        errors = self.errors(self.files_page(""))
        self.assertIn("docs/reference/files.md: The table of the authority classes is empty", errors)
        self.assertIn("Floor: no file was given an authority class", errors)

    def test_the_command_fails_on_a_file_without_a_class(self):
        files = self.files_page(FILES_ROWS)
        files["Cargo.lock"] = "version = 4\n"
        status, output = self.run_main(self.fixture(files))
        self.assertEqual(status, 1)
        self.assertIn("ERROR: Cargo.lock: No authority class; add a row to docs/reference/files.md", output)
        status, output = self.run_main(self.fixture(self.files_page(FILES_ROWS)))
        self.assertEqual(status, 0)
        self.assertIn("7 files with an authority class", output)

    def test_a_directory_that_is_no_repository_gives_each_file_below_it(self):
        root = self.fixture({"target/built.md": "skipped\n", "a/b/c.txt": "x\n", "a/target/d.txt": "x\n"})
        files = committed_files(root)
        self.assertIn("a/b/c.txt", files)
        self.assertIn("docs/log.md", files)
        # A build directory is left out at the top only, as for the Markdown files.
        self.assertNotIn("target/built.md", files)
        self.assertIn("a/target/d.txt", files)


class AuthorityClassGitTests(FixtureTests):
    """The gate reads the index of git: what is committed, and what is staged."""

    def git(self, root, *arguments):
        import os
        import subprocess

        # Inside a git hook these variables name the index of the project; the test works on
        # the repository that it makes, and on no other.
        environment = {
            name: value for name, value in os.environ.items()
            if name not in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_PREFIX")
        }
        return subprocess.run(
            ["git", "-C", str(root), *arguments], capture_output=True, check=True, env=environment
        )

    def repository(self):
        root = self.fixture({FILES_PATH: FILES})
        self.git(root, "init", "--quiet")
        self.git(root, "add", "--all")
        return root

    def test_a_staged_file_is_checked_and_a_file_that_is_not_staged_is_not(self):
        root = self.repository()
        self.assertEqual(validate(root, TODAY), [])
        self.assertEqual(len(committed_files(root)), 7)
        (root / "new.txt").write_text("x\n", encoding="utf-8")
        self.assertEqual(validate(root, TODAY), [])
        self.assertNotIn("new.txt", committed_files(root))
        self.git(root, "add", "new.txt")
        self.assertIn("new.txt", committed_files(root))
        self.assertEqual(validate(root, TODAY), ["new.txt: No authority class; add a row to docs/reference/files.md"])
        # The entry counts also when the file is gone from the working tree again.
        (root / "new.txt").unlink()
        self.assertEqual(validate(root, TODAY), ["new.txt: No authority class; add a row to docs/reference/files.md"])

    def test_a_file_that_git_ignores_has_no_class_to_check(self):
        root = self.fixture({FILES_PATH: FILES, ".gitignore": "secret.txt\n", "secret.txt": "x\n"})
        page = FILES + "| `.gitignore` | Authored | What git does not track. |\n"
        (root / FILES_PATH).write_text(page, encoding="utf-8")
        self.git(root, "init", "--quiet")
        self.git(root, "add", "--all")
        self.assertEqual(validate(root, TODAY), [])
        self.assertNotIn("secret.txt", committed_files(root))

    def test_a_repository_that_git_cannot_read_fails_the_gate(self):
        root = self.repository()
        (root / ".git" / "index").write_bytes(b"not an index")
        self.assertIsNone(committed_files(root))
        errors = validate(root, TODAY)
        self.assertIn("docs/reference/files.md: git cannot list the files of the repository", errors)
        self.assertIn("Floor: no file was given an authority class", errors)

    def test_a_missing_git_command_fails_the_gate_with_the_same_message(self):
        import os
        from unittest import mock

        root = self.repository()
        with mock.patch.dict(os.environ, {"PATH": ""}):
            self.assertIsNone(committed_files(root))
            self.assertIn("docs/reference/files.md: git cannot list the files of the repository", validate(root, TODAY))


class RealRepositoryAuthorityTests(unittest.TestCase):
    def test_git_lists_the_files_of_the_repository_and_each_has_a_class(self):
        files = committed_files(ROOT)
        self.assertIn("Cargo.lock", files)
        self.assertIn("scripts/check_docs.py", files)
        # The list is the index of git, not a walk of the tree: git gives the same names.
        import subprocess

        listed = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z"], capture_output=True, check=True)
        self.assertEqual(files, sorted(name for name in listed.stdout.decode().split("\0") if name))
        counts = check(ROOT).counts
        self.assertEqual(counts.files_with_a_class, len(files))
        self.assertGreaterEqual(counts.files_with_a_class, 150)

    def test_the_lockfile_and_the_vendored_standard_are_derived_and_the_log_is_evidence(self):
        text = (ROOT / "docs/reference/files.md").read_text(encoding="utf-8")
        classes = {}
        rows = authority_rows(text)
        for name in ("Cargo.lock", "standards/reas/standard/index.md", "standards/reas/README.md", "docs/log.md", "docs/index.md", "src/main.rs", "README.md"):
            for cell, authority, _ in rows:
                pattern = cell.strip("`")
                if name == pattern or (pattern.endswith("/") and name.startswith(pattern)):
                    classes[name] = authority
                    break
        self.assertEqual(classes, {
            "Cargo.lock": "Derived, committed",
            "standards/reas/standard/index.md": "Derived, committed",
            "standards/reas/README.md": "Authored",
            "docs/log.md": "Evidence",
            "docs/index.md": "Authored",
            "src/main.rs": "Authored",
            "README.md": "Companion",
        })


if __name__ == "__main__":
    unittest.main()
