#!/usr/bin/env python3
"""Keep the i18next locale catalogs in step with the strings the source actually uses.

  scripts/sync-localizations.py            mirror en to en-GB, report on fr
  scripts/sync-localizations.py --check    report only, non-zero exit if anything is off (CI)

en-GB is kept as a mirror of en: every key copies its en value verbatim except the ones listed in
EN_GB_OVERRIDES, which carry deliberately different British spelling or phrasing. Nothing in the
current copy diverges from American spelling, so EN_GB_OVERRIDES is empty for now; add an entry
there the day a string needs to.

fr has no machine-generated content: a human writes every value, and this script only checks that
every en key exists in it, that it carries nothing en has dropped, and that every plural key in
every catalog has both halves of its pair.
"""
import argparse
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "app/src"
EN = ROOT / "app/src/locales/en/common.json"
EN_GB = ROOT / "app/src/locales/en-GB/common.json"
FR = ROOT / "app/src/locales/fr/common.json"

PLURAL_SUFFIXES = ("_one", "_other")

KEY_RE = re.compile(r"""(['"])([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*)+)\1""")
TEMPLATE_PREFIX_RE = re.compile(r"""t\(\s*`([A-Za-z][A-Za-z0-9.]*)\$\{""")

EN_GB_OVERRIDES: dict[str, str] = {}


def flatten(node, prefix=""):
    for key, value in node.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            yield from flatten(value, path)
        else:
            yield path, value


def base_key(path):
    for suffix in PLURAL_SUFFIXES:
        if path.endswith(suffix):
            return path[: -len(suffix)]
    return path


def is_test_path(path):
    """Test files never call t(), and their fixtures are ordinary strings that can happen to look
    exactly like a dotted key."""
    if ".test." in path.name or ".spec." in path.name:
        return True
    return any(part in ("__tests__", "__mocks__") for part in path.parts)


def scan_source():
    """Every key-shaped string literal in app/src, plus the static prefix of any template literal
    passed to t(). Key references are not always a direct t("literal") call: sections.ts and
    similar files hand a labelKey/titleKey string through a prop or a switch, and the call site
    reads it back out of a variable, so this scans for the literal key text wherever it appears
    in source rather than only inside t(...)."""
    used = set()
    prefixes = set()
    for path in SOURCE.rglob("*"):
        if path.suffix not in (".ts", ".tsx") or path.name == "bindings.ts":
            continue
        if is_test_path(path):
            continue
        text = path.read_text()
        used.update(match.group(2) for match in KEY_RE.finditer(text))
        prefixes.update(match.group(1) for match in TEMPLATE_PREFIX_RE.finditer(text))
    return used, prefixes


def is_referenced(path, used, prefixes):
    if path in used:
        return True
    base = base_key(path)
    if base in used:
        return True
    return any(base.startswith(prefix) for prefix in prefixes)


def check_extraction(en):
    catalog_paths = [path for path, _ in flatten(en)]
    catalog_bases = {base_key(path) for path in catalog_paths}
    used, prefixes = scan_source()

    missing = sorted(key for key in used if key not in catalog_paths and key not in catalog_bases)
    unused = sorted(path for path in catalog_paths if not is_referenced(path, used, prefixes))
    return missing, unused


def mirror(node, overrides, prefix=""):
    result = {}
    for key, value in node.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            result[key] = mirror(value, overrides, path)
        else:
            result[key] = overrides.get(path, value)
    return result


def write_json(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n")


def check_fr(en):
    en_paths = {path for path, _ in flatten(en)}
    fr = json.loads(FR.read_text()) if FR.exists() else {}
    fr_paths = {path for path, _ in flatten(fr)}
    missing = sorted(en_paths - fr_paths)
    stale = sorted(fr_paths - en_paths)
    return missing, stale


def check_plural_pairs(label, catalog):
    """i18next picks _one or _other at render time, so a key with only one half renders its own
    name as the string the day the count goes the other way."""
    paths = {path for path, _ in flatten(catalog)}
    broken = []
    for path in sorted(paths):
        for suffix, twin in (("_one", "_other"), ("_other", "_one")):
            if path.endswith(suffix):
                partner = path[: -len(suffix)] + twin
                if partner not in paths:
                    broken.append(f"{label}: {path} has no {partner}")
    return broken


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="report only, never write")
    args = parser.parse_args()

    en = json.loads(EN.read_text())
    problems = []

    missing, unused = check_extraction(en)
    if missing:
        problems.append(
            f"{len(missing)} key(s) used in app/src but missing from en/common.json: "
            + ", ".join(missing)
        )
    if unused:
        problems.append(
            f"{len(unused)} key(s) in en/common.json are never used in app/src: "
            + ", ".join(unused)
        )

    expected_en_gb = mirror(en, EN_GB_OVERRIDES)
    actual_en_gb = json.loads(EN_GB.read_text()) if EN_GB.exists() else None
    if expected_en_gb != actual_en_gb:
        if args.check:
            problems.append("en-GB/common.json is out of sync with en/common.json")
        else:
            write_json(EN_GB, expected_en_gb)
            print(f"wrote {EN_GB.relative_to(ROOT)}")

    fr_missing, fr_stale = check_fr(en)
    if fr_missing:
        problems.append(
            f"fr/common.json is missing {len(fr_missing)} key(s): " + ", ".join(fr_missing)
        )
    if fr_stale:
        problems.append(
            f"fr/common.json has {len(fr_stale)} key(s) not in en/common.json: "
            + ", ".join(fr_stale)
        )

    for label, path in (("en", EN), ("en-GB", EN_GB), ("fr", FR)):
        catalog = json.loads(path.read_text()) if path.exists() else {}
        problems.extend(check_plural_pairs(label, catalog))

    if problems:
        sys.exit("\n".join(problems))

    total = len(list(flatten(en)))
    print(f"{total} string(s) across en, en-GB, and fr, all in sync")


if __name__ == "__main__":
    main()
