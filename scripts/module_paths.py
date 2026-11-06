#!/usr/bin/env python3
"""The module-path gate (FD-043-D35 and D37, T15.6): no function is reached through an imported
module of genie's, and no path goes through `super`.

    scripts/module_paths.py        fails on `module::function` where `use …::module;` imported
                                   a module of genie's, as `file:line: module::function`, and on
                                   a path through `super` but a test module's `use super::*;`,
                                   as `file:line: super::path`

`use genie_math::fmath;` then `fmath::sqrt(x)` names the function through the module; the rule
(T15.5's crate-root imports, which clippy's `absolute_paths` cannot see for a module imported
by `use`) wants `use genie_math::fmath::sqrt;` and `sqrt(x)`. A module of genie's is one a
`use` reaches from `crate`, `self`, `super` or a genie crate (`genie_…`); std's and other
crates' modules keep Rust's idiom (`mem::take`, `fs::read`, two segments as clippy's
`absolute_paths` allows). A type or a constant through a module (`layers::Action`, the tests'
`class::GATE`) is not a function: the module names its space (FD-045-D59, FD-046-D43).

An import goes from the crate root (CODING_STYLE.md section 4): `use crate::generated::world::
{Object, Player};`, not `use super::generated::world::{…}`, and `use crate::written_len;`, then
`written_len(x)`, not `super::written_len(x)`. A test module's `use super::*;` is the one
exception. A generator writes its imports the same way.

It reads the code, not the comments or the strings, of every `.rs` file git knows of, generated
ones too.

Python 3.11 or later, no packages.
"""

import re
import sys
from pathlib import Path

from file_size import line_of, mask, rust_files

USE = re.compile(r"\buse\s+([^;]+);")
PATH_USE = re.compile(r"\b([a-z_][a-z0-9_]*)\s*::")
SUPER = re.compile(r"\bsuper\s*::")
SUPER_GLOB = re.compile(r"\buse\s+(super)\s*::\s*\*\s*;")
SUPER_PATH = re.compile(r"super(?:\s*::\s*(?:[A-Za-z_][A-Za-z0-9_]*|\{))*")


def imported_modules(code: str) -> set[str]:
    """The names of `imported(code)` that come from genie's own code."""
    return {name for name, root in imported(code) if root in {"crate", "self", "super"} or root.startswith("genie_")}


def imported(code: str) -> set[tuple[str, str]]:
    """The lower-case names the file's `use` declarations import that a path can go through:
    the last segment of a path (`use crate::math::fmath;`), a name in braces (`use
    std::{fmt, io};`), the module of a `self` (`use std::io::{self, Read};`) and an alias
    (`use genie_rec::de as rec;`)."""
    names: set[tuple[str, str]] = set()
    for m in USE.finditer(code):
        tree = re.sub(r"\s+", " ", m.group(1)).strip()
        root = tree.split("::")[0].strip("{ ")
        names |= {(name, root) for name in leaves(tree, "") if name != root}
    return names


def leaves(tree: str, parent: str) -> set[str]:
    """The names `tree` (a use tree, without `use` and `;`) brings into scope, `parent` the last
    segment before it."""
    tree = tree.strip()
    if not tree:
        return set()
    brace = tree.find("{")
    if brace >= 0 and tree.endswith("}"):
        prefix = tree[:brace].rstrip(": ")
        last = prefix.split("::")[-1] if prefix else parent
        out: set[str] = set()
        for part in split_top(tree[brace + 1 : -1]):
            out |= leaves(part, last)
        return out
    alias = re.match(r"(.*)\s+as\s+([A-Za-z_][A-Za-z0-9_]*)$", tree)
    if alias:
        name = alias.group(2)
        return {name} if name != "_" and name[:1].islower() else set()
    last = tree.split("::")[-1].strip()
    if last == "self":
        segments = tree.split("::")
        last = segments[-2].strip() if len(segments) > 1 else parent
    if last == "*" or not last[:1].islower() or last in {"self", "super", "crate"}:
        return set()
    return {last}


def split_top(s: str) -> list[str]:
    """`s` split at its top-level commas."""
    out, depth, start = [], 0, 0
    for i, c in enumerate(s):
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
        elif c == "," and depth == 0:
            out.append(s[start:i])
            start = i + 1
    out.append(s[start:])
    return out


def problems(src: str) -> list[tuple[int, str]]:
    """Each path of `src` through a module its `use` declarations import, as (line, path)."""
    code = mask(src)
    modules = imported_modules(code)
    uses = {m.span() for m in USE.finditer(code)}
    found = []
    for m in PATH_USE.finditer(code):
        name = m.group(1)
        if name not in modules:
            continue
        if any(a <= m.start() < b for a, b in uses):
            continue  # the import itself (`use fmath::{…}` after a `use …::fmath;` is a path too)
        before = code[: m.start()].rstrip()
        if before.endswith("::"):
            continue  # a segment inside a longer path (`crate::math::fmath::sqrt`)
        rest = code[m.end() :]
        item = re.match(r"\s*([a-z_][a-z0-9_]*)\b(?!\s*::)", rest)
        if not item:
            continue  # a type, a constant or a deeper module path
        found.append((line_of(src, m.start()), f"{name}::{item.group(1)}"))
    return found


def super_paths(src: str) -> list[tuple[int, str]]:
    """Each path of `src`'s code through `super` but the glob of `use super::*;`, as (line, the
    path up to its last name or its brace: `super::written_len`, `super::super::{`)."""
    code = mask(src)
    globs = {m.start(1) for m in SUPER_GLOB.finditer(code)}
    found = []
    end = 0
    for m in SUPER.finditer(code):
        if m.start() in globs or m.start() < end:
            continue  # the glob, or the second `super` of `super::super::…`
        path = SUPER_PATH.match(code, m.start())
        end = path.end() if path else m.end()
        found.append((line_of(src, m.start()), re.sub(r"\s+", "", code[m.start() : end])))
    return found


def main() -> int:
    files = [f for f in rust_files() if "/target" not in f.as_posix()]
    sources = {f: f.read_text() for f in files}
    through_modules = [(f, *p) for f, src in sources.items() for p in problems(src)]
    through_super = [(f, *p) for f, src in sources.items() for p in super_paths(src)]
    for f, line, path in through_modules:
        print(f"{f}:{line}: {path}: a function through an imported module (FD-043-D37: import it)")
    for f, line, path in through_super:
        print(f"{f}:{line}: {path}: a path through super (CODING_STYLE.md section 4: import from the crate root)")
    if through_modules or through_super:
        print(
            f"module-paths: {len(through_modules)} functions are reached through an imported module, "
            f"{len(through_super)} paths go through super"
        )
        return 1
    print(f"module-paths: no function of {len(files)} files is reached through an imported module or super")
    return 0


if __name__ == "__main__":
    sys.exit(main())
