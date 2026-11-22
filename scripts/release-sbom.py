#!/usr/bin/env python3
"""Generate a CycloneDX 1.5 SBOM for a release archive from Cargo.lock.

Only the Python standard library is used, and the whole lockfile is read with a
fixed size bound. The output has no timestamp, so identical inputs produce
byte-identical JSON.

Usage:
    python3 scripts/release-sbom.py \
        --archive dist/name.tar.gz --name name --version v1.2.3 \
        --target x86_64-unknown-linux-gnu --output dist/name.cdx.json

Run ``--self-test`` for the built-in checks.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import quote

MAX_LOCK_BYTES = 16 * 1024 * 1024
NAME_RE = re.compile(r'^name = "([^"]+)"$')
VERSION_RE = re.compile(r'^version = "([^"]+)"$')


def parse_cargo_lock(text: str) -> list[tuple[str, str]]:
    """Return unique (name, version) packages, sorted by name then version."""
    packages: set[tuple[str, str]] = set()
    current: dict[str, str] | None = None

    def finish() -> None:
        nonlocal current
        if current is None:
            return
        if "name" not in current or "version" not in current:
            raise ValueError("a [[package]] block is missing name or version")
        packages.add((current["name"], current["version"]))
        current = None

    for raw in text.splitlines():
        line = raw.strip()
        if line == "[[package]]":
            finish()
            current = {}
            continue
        if line.startswith("["):
            finish()
            continue
        if current is None:
            continue
        match = NAME_RE.match(line)
        if match:
            current["name"] = match.group(1)
            continue
        match = VERSION_RE.match(line)
        if match:
            current["version"] = match.group(1)
    finish()

    if not packages:
        raise ValueError("no packages found in the lockfile")
    return sorted(packages)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def purl(name: str, version: str) -> str:
    return "pkg:cargo/{}@{}".format(
        quote(name, safe="-_."),
        quote(version, safe=".-_"),
    )


def build_sbom(
    packages: list[tuple[str, str]],
    *,
    name: str | None = None,
    version: str | None = None,
    archive: Path | None = None,
    target: str | None = None,
) -> dict:
    bom: dict = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {
            "tools": [
                {
                    "vendor": "Tech-Cred",
                    "name": "release-sbom.py",
                    "version": "1",
                }
            ]
        },
        "components": [
            {
                "type": "library",
                "name": dep_name,
                "version": dep_version,
                "purl": purl(dep_name, dep_version),
            }
            for dep_name, dep_version in packages
        ],
    }

    if archive is not None:
        if name is None:
            raise ValueError("--name is required with --archive")
        if not archive.is_file():
            raise ValueError(f"archive not found: {archive}")
        component: dict = {
            "type": "application",
            "name": name,
            "hashes": [{"alg": "SHA-256", "content": sha256_file(archive)}],
        }
        if version is not None:
            component["version"] = version
        if target is not None:
            component["properties"] = [
                {"name": "observatory:target", "value": target}
            ]
        bom["metadata"]["component"] = component

    return bom


def render_sbom(bom: dict) -> str:
    return json.dumps(bom, indent=2, ensure_ascii=True) + "\n"


def self_test() -> None:
    lock = """\
version = 3

[[package]]
name = "zeta"
version = "2.0.0"

[[package]]
name = "alpha"
version = "1.0.0"

[[package]]
name = "alpha"
version = "0.9.0"
"""
    packages = parse_cargo_lock(lock)
    assert packages == [
        ("alpha", "0.9.0"),
        ("alpha", "1.0.0"),
        ("zeta", "2.0.0"),
    ], packages
    bom = build_sbom(packages)
    assert bom["bomFormat"] == "CycloneDX"
    assert bom["specVersion"] == "1.5"
    assert [component["name"] for component in bom["components"]] == [
        "alpha",
        "alpha",
        "zeta",
    ]
    assert bom["components"][0]["purl"] == "pkg:cargo/alpha@0.9.0"
    assert render_sbom(bom) == render_sbom(build_sbom(parse_cargo_lock(lock)))
    try:
        parse_cargo_lock("[[package]]\nname = \"broken\"\n")
    except ValueError:
        pass
    else:
        raise AssertionError("malformed package block was accepted")
    print("ok")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, default=Path("Cargo.lock"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--name")
    parser.add_argument("--version")
    parser.add_argument("--target")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        self_test()
        return 0
    if args.output is None:
        parser.error("--output is required")

    lock_bytes = args.lock.read_bytes()
    if len(lock_bytes) > MAX_LOCK_BYTES:
        raise SystemExit(f"error: {args.lock} exceeds {MAX_LOCK_BYTES} bytes")
    packages = parse_cargo_lock(lock_bytes.decode("utf-8"))
    bom = build_sbom(
        packages,
        name=args.name,
        version=args.version,
        archive=args.archive,
        target=args.target,
    )
    args.output.write_text(render_sbom(bom), encoding="utf-8")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (OSError, ValueError) as error:
        raise SystemExit(f"error: {error}")
