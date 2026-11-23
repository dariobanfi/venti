#!/usr/bin/env python3
"""Writes licences/core-notices.txt: the notices of the crates compiled into
the apps' Rust core, which their licences ask a binary to carry.

    export PATH=/opt/homebrew/opt/rustup/bin:$PATH   # on the owner's Mac
    python3 tools/licences/notices.py           # write the file
    python3 tools/licences/notices.py --check   # fail if it is not in step

Which crates: what `cargo tree` lists as normal dependencies of each build
in BUILDS, for its targets, with the features that build resolves; crates
of the workspace and proc macros (they run on the build machine and are not
in the binary) are left out. What a crate is: `cargo metadata`. Where a
crate offers a choice of licences, it is taken under the first of
`deny.toml`'s allowed licences that the choice offers; a licence outside
that list stops the script.

The text of each licence is given once, from the crates' own licence files:
the wording most of them share, with its copyright lines taken out. A crate
whose own wording differs carries it in full under its name. A licence no
crate ships a file of takes the standard text in tools/licences/texts.
Standard library only; the cargo is $CARGO, else `cargo` on the PATH.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
OUTPUT = REPOSITORY / "licences" / "core-notices.txt"
TEXTS = Path(__file__).resolve().parent / "texts"

# What the apps ship and the targets each is built for:
# apps/ios/scripts/build-core.sh (the XCFramework's slices),
# apps/android/gradle.properties `venti.abis` and the ABIs Android names
# (the shared library), apps/web/scripts/build-core.sh (the module).
BUILDS = [
    (
        "venti-core",
        [
            "aarch64-apple-ios",
            "aarch64-apple-ios-sim",
            "x86_64-apple-ios",
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-linux-android",
            "armv7-linux-androideabi",
            "i686-linux-android",
            "x86_64-linux-android",
        ],
    ),
    ("venti-core-wasm", ["wasm32-unknown-unknown"]),
]

# What each licence's text says that no other's does, after normalize().
FINGERPRINTS = [
    ("Apache-2.0 WITH LLVM-exception", ["llvmexceptionstotheapache20license"]),
    ("Apache-2.0", ["apachelicense", "version20", "termsandconditionsforusereproductionanddistribution"]),
    ("MPL-2.0", ["mozillapubliclicenseversion20"]),
    ("Unicode-3.0", ["unicodelicensev3"]),
    ("MIT", ["permissionisherebygrantedfreeofchargetoanypersonobtainingacopyofthissoftware"]),
    ("Zlib", ["thissoftwareisprovidedasis", "alteredsourceversionsmustbeplainlymarked"]),
    ("BSD-3-Clause", ["redistributionsinbinaryformmustreproduce", "neitherthenameof"]),
    ("ISC", ["permissiontousecopymodifyandordistributethissoftwareforanypurposewithorwithoutfeeisherebygrantedprovidedthat"]),
    ("Unlicense", ["thisisfreeandunencumberedsoftwarereleasedintothepublicdomain"]),
]

LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|copyright|notice|unlicense)", re.IGNORECASE)
COPYRIGHT_LINE = re.compile(r"^\s*(?:[#*/;]+\s*)?(Copyright(?:\s|\(|©|:|,).*?)\s*$")
PLACEHOLDER = re.compile(r"\[yyyy\]|\{yyyy\}|<year>|<copyright holders>|\[name of copyright owner\]", re.IGNORECASE)


def fail(message):
    print(f"notices.py: {message}", file=sys.stderr)
    sys.exit(2)


def cargo(*args):
    """Runs cargo offline first, from the local registry, and online if that fails."""
    program = os.environ.get("CARGO", "cargo")
    for extra in (["--offline"], []):
        result = subprocess.run(
            [program, *args, "--locked", *extra],
            cwd=REPOSITORY,
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            return result.stdout
    fail(f"cargo {' '.join(args)} failed:\n{result.stderr}")


def shipped(packages, workspace_names):
    """The (name, version) of every crate of others in a binary the apps ship."""
    found = set()
    for root, targets in BUILDS:
        arguments = ["tree", "-p", root, "-e", "normal,no-proc-macro", "--prefix", "none", "-f", "{p}"]
        for target in targets:
            arguments += ["--target", target]
        for line in cargo(*arguments).splitlines():
            words = line.split()
            if len(words) >= 2 and words[1].startswith("v") and words[0] not in workspace_names:
                found.add((words[0], words[1][1:]))
    unknown = found - set(packages)
    if unknown:
        fail(f"cargo tree names crates cargo metadata does not: {sorted(unknown)}")
    return found


def normalize(text):
    return re.sub(r"[^a-z0-9]", "", text.lower())


def read(path):
    return path.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")


def classify(text):
    """The one licence a file holds the text of, or None for none or several."""
    flat = normalize(text)
    found = [spdx for spdx, marks in FINGERPRINTS if all(mark in flat for mark in marks)]
    if "Apache-2.0 WITH LLVM-exception" in found:
        found.remove("Apache-2.0")
    return found[0] if len(found) == 1 else None


def copyright_lines(text):
    lines = []
    for line in text.split("\n"):
        match = COPYRIGHT_LINE.match(line)
        if match and not PLACEHOLDER.search(line):
            lines.append(match.group(1))
    return lines


def without_copyright(text):
    """The text with its copyright lines out and runs of blank lines made one."""
    kept = [line.rstrip() for line in text.split("\n") if not (COPYRIGHT_LINE.match(line) and not PLACEHOLDER.search(line))]
    return re.sub(r"\n{3,}", "\n\n", "\n".join(kept)).strip("\n")


def wording(text):
    """What two copies of a licence are compared by: no copyright lines, no title
    such as "The MIT License (MIT)", and nothing but letters and digits."""
    lines = without_copyright(text).split("\n")
    if lines and re.search(r"licen[cs]e", lines[0], re.IGNORECASE) and len(lines[0].split()) <= 6 and not lines[0].rstrip().endswith("."):
        lines.pop(0)
    return normalize("\n".join(lines))


def parse(expression):
    """An SPDX expression as a tree: a licence name, ("OR", [...]) or ("AND", [...])."""
    tokens = re.findall(r"\(|\)|[^\s()]+", expression.replace("/", " OR "))
    position = 0

    def peek():
        return tokens[position] if position < len(tokens) else None

    def take():
        nonlocal position
        position += 1
        return tokens[position - 1]

    def term():
        if peek() == "(":
            take()
            node = either()
            if take() != ")":
                fail(f"unbalanced licence expression {expression!r}")
            return node
        name = take()
        if peek() == "WITH":
            take()
            name = f"{name} WITH {take()}"
        return name

    def joined(operator, inner):
        parts = [inner()]
        while peek() == operator:
            take()
            parts.append(inner())
        return parts[0] if len(parts) == 1 else (operator, parts)

    def both():
        return joined("AND", term)

    def either():
        return joined("OR", both)

    tree = either()
    if position != len(tokens):
        fail(f"cannot read licence expression {expression!r}")
    return tree


def elect(tree, preference):
    """The licences a crate is taken under, or None where the choice offers no allowed one."""
    if isinstance(tree, str):
        return [tree] if tree in preference else None
    operator, parts = tree
    chosen = [elect(part, preference) for part in parts]
    if operator == "AND":
        if any(choice is None for choice in chosen):
            return None
        return sorted({name for choice in chosen for name in choice}, key=preference.index)
    options = [choice for choice in chosen if choice is not None]
    if not options:
        return None
    return min(options, key=lambda choice: sorted(preference.index(name) for name in choice))


def standard_text(spdx):
    path = TEXTS / f"{spdx}.txt"
    if not path.exists():
        fail(f"no crate ships the text of {spdx} and {path.relative_to(REPOSITORY)} is missing: add the standard text there")
    lines = read(path).split("\n")
    while lines and lines[0].startswith("#"):
        lines.pop(0)
    return "\n".join(line.rstrip() for line in lines).strip("\n")


def generate():
    preference = tomllib.loads((REPOSITORY / "deny.toml").read_text())["licenses"]["allow"]
    metadata = json.loads(cargo("metadata", "--format-version", "1"))
    workspace = set(metadata["workspace_members"])
    packages = {}
    for package in metadata["packages"]:
        if package["id"] in workspace:
            continue
        key = (package["name"], package["version"])
        if key in packages:
            fail(f"two packages are {key[0]} {key[1]}")
        packages[key] = package
    workspace_names = {package["name"] for package in metadata["packages"] if package["id"] in workspace}

    crates = []
    for key in sorted(shipped(packages, workspace_names)):
        package = packages[key]
        if not package.get("license"):
            fail(f"{key[0]} {key[1]} states no SPDX licence expression")
        elected = elect(parse(package["license"]), preference)
        if elected is None:
            fail(f"{key[0]} {key[1]} is under {package['license']}, which offers no licence deny.toml allows")
        folder = Path(package["manifest_path"]).parent
        files = sorted(path for path in folder.iterdir() if path.is_file() and LICENCE_FILE.match(path.name))
        if package.get("license_file"):
            extra = folder / package["license_file"]
            if extra.is_file() and extra not in files:
                files.append(extra)
        texts = {}
        copyrights = []
        for path in files:
            text = read(path)
            for line in copyright_lines(text):
                if line not in copyrights:
                    copyrights.append(line)
            spdx = classify(text)
            if spdx in elected and spdx not in texts:
                texts[spdx] = (path.name, text)
        crates.append(
            {
                "name": key[0],
                "version": key[1],
                "expression": package["license"],
                "elected": elected,
                "copyrights": copyrights,
                "authors": package.get("authors") or [],
                "repository": package.get("repository"),
                "registry": (package.get("source") or "").startswith(("registry+", "sparse+")),
                "texts": texts,
            }
        )

    # One wording per licence: the one most crates share.
    families = {}
    for spdx in sorted({name for crate in crates for name in crate["elected"]}, key=preference.index):
        groups = defaultdict(list)
        for crate in crates:
            if spdx in crate["texts"]:
                file, text = crate["texts"][spdx]
                groups[wording(text)].append((crate["name"], crate["version"], file, without_copyright(text)))
        if groups:
            flat, members = min(groups.items(), key=lambda item: (-len(item[1]), item[0]))
            name, version, file, body = members[0]
            families[spdx] = {"flat": flat, "body": body, "from": f"the wording of {name} {version}'s {file}, its copyright lines taken out"}
        else:
            families[spdx] = {"flat": None, "body": standard_text(spdx), "from": "the standard text: no crate below ships a file of it"}

    out = []
    title = "Third-party software in Venti's core"
    out += [title, "=" * len(title), ""]
    out += [
        "The Venti apps for iPhone, Android and the web carry a core written in",
        "Rust, and it is built with the crates listed here, the work of others.",
        "Each is named with its version, its licence and the copyright lines of",
        "its licence files. Where a crate offers a choice of licences, the one",
        "Venti takes it under is named. The text of each licence follows the",
        "list, once.",
        "",
        "This file is generated by tools/licences/notices.py from Cargo.lock and",
        "the crates' own files; do not edit it. To write it again:",
        "    python3 tools/licences/notices.py",
        "and to check that it is in step: python3 tools/licences/notices.py --check",
        "",
        f"{len(crates)} crates under {', '.join(families)}.",
        "",
        "",
    ]
    heading = "Crates"
    out += [heading, "-" * len(heading), ""]
    for crate in crates:
        out.append(f"{crate['name']} {crate['version']}")
        licence = crate["expression"]
        chosen = " AND ".join(crate["elected"])
        out.append(f"  Licence: {licence}" + ("" if normalize(licence) == normalize(chosen) else f", taken under {chosen}"))
        if crate["registry"]:
            out.append(f"  Source: https://crates.io/crates/{crate['name']}/{crate['version']}")
        if crate["repository"]:
            out.append(f"  Repository: {crate['repository']}")
        if crate["copyrights"]:
            out += [f"  {line}" for line in crate["copyrights"]]
        else:
            authors = ", ".join(crate["authors"]) or "none named"
            out.append(f"  No copyright line in its licence files; authors in its Cargo.toml: {authors}")
        for spdx in crate["elected"]:
            if spdx in crate["texts"]:
                file, text = crate["texts"][spdx]
                if wording(text) != families[spdx]["flat"]:
                    out.append(f"  Its own text of {spdx}, from {file}, differs from the one below:")
                    out += [f"    {line}".rstrip() for line in text.strip("\n").split("\n")]
        out.append("")
    out.append("")
    heading = "Licence texts"
    out += [heading, "-" * len(heading), ""]
    for spdx, family in families.items():
        bar = "=" * 72
        out += [bar, spdx, f"({family['from']})", bar, "", family["body"], "", ""]
    return "\n".join(line.rstrip() for line in out).rstrip("\n") + "\n", crates, families


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="exit 1 when the committed file differs from a fresh run")
    arguments = parser.parse_args()
    text, crates, families = generate()
    relative = OUTPUT.relative_to(REPOSITORY)
    if arguments.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else None
        if current != text:
            print(f"{relative} is not in step with Cargo.lock: run python3 tools/licences/notices.py", file=sys.stderr)
            sys.exit(1)
        print(f"{relative} is in step: {len(crates)} crates.")
        return
    OUTPUT.parent.mkdir(exist_ok=True)
    OUTPUT.write_text(text, encoding="utf-8")
    print(f"Wrote {relative}: {len(crates)} crates, {len(text.encode())} bytes, licences {', '.join(families)}.")


if __name__ == "__main__":
    main()
