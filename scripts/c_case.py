#!/usr/bin/env python3
"""The casing gate (CODING_STYLE.md section 3, C-CASE to the letter, FD-043): every name in
genie's Rust code follows RFC 430.

    scripts/c_case.py        fails on a name that breaks it, as `file:line: name: why`

It reads the code, not the comments or the strings, of every `.rs` file git knows of, the
generated ones too (their generators follow the rule), and checks four rules:

- A type or a variant (`struct`, `enum`, `trait`, `type`, `union`, an enum's variant) holds no
  acronym in capitals: no run of three capitals (`AIModule`) and no two at the end
  (`UnitKindID`). A single letter is a word in `UpperCamelCase` (`XAxis`, `BVector`), so a
  plural acronym (`IDs`) passes: the review catches it.
- In a `snake_case` or `SCREAMING_SNAKE_CASE` name a single letter is a word only at the end:
  `map_x`, not `t_master` or `a_build_s_walk`. A digit is no letter (`state_6_goes_on`).
- An enum's values are its variants: no associated constant of the enum's own type
  (`const MOVE: Self = Self::Move`) in its crate. A constant of another type (`ALL: [Self;
  4]`) stays, and so do a newtype's constants (`GameMode::REGICIDE`).
- Kind, not Type (FD-043-D33): no name holds the word `type`, a Rust keyword, nor `types` or a
  numbered `type4`: `PlayerKind`, `unit_kind`, `KIND_2`. The keyword itself passes, and so do
  the names other crates and published formats define (`TypeId`, `type_name`, syn's `Type`,
  clippy's `type_complexity` and `disallowed_types`, genieutils' `Type50`).

Python 3.11 or later, no packages.
"""

import re
import sys
from pathlib import Path

from file_size import line_of, mask, rust_files

TYPE_DECL = re.compile(r"\b(?:struct|enum|trait|type|union)\s+([A-Za-z_][A-Za-z0-9_]*)")
ENUM_DECL = re.compile(r"\benum\s+([A-Za-z_][A-Za-z0-9_]*)[^{;]*\{")
VARIANT = re.compile(r"(?:^|[{,])\s*(?:#\s*\[[^\]]*\]\s*)*([A-Z][A-Za-z0-9_]*)\b")
IDENT = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
IMPL = re.compile(r"\bimpl(?:\s*<[^>]*>)?\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:<[^>]*>)?\s*\{")
SELF_CONST = r"\bconst\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*(?:Self|{name})\s*="
CAPITALS = re.compile(r"[A-Z]{3,}|[a-z0-9][A-Z]{2,}$|^[A-Z]{2,}$")
# the word `type` in a snake or screaming name, or in an UpperCamelCase one
TYPE_WORD = re.compile(r"(?:^|_)(?:type|TYPE)(?:s|S|\d+)?(?=_|$)|Type(?:s|\d+)?(?![a-z])")
OUTSIDE_TYPE_NAMES = {
    "type",  # the keyword (`type Output = …`, `r#type`)
    "Type",  # syn's
    "TypeId",
    "TypePath",
    "TypeParam",
    "TypeGenerics",
    "TypeReference",
    "TypeTuple",
    "type_name",
    "type_name_of_val",
    "type_params",
    "type_params_mut",
    "type_generics",
    "type_complexity",
    "disallowed_types",  # clippy's lint (`clippy.toml`'s `disallowed-types`)
    "type_repetition_in_bounds",
    "Type50",  # genieutils' name of the `.dat` unit's combat part
    "type50",
}


def camel_problem(name: str) -> str | None:
    """Why the `UpperCamelCase` `name` breaks C-CASE, or None."""
    if "_" in name.strip("_") or not name[:1].isupper():
        return None  # not an UpperCamelCase name (rustc's nonstandard_style judges it)
    if CAPITALS.search(name) and len(name) > 1:
        return "an acronym in capitals: it is one word (`UnitTypeId`, `AiModule`)"
    return None


def kind_problem(name: str) -> str | None:
    """Why `name` breaks Kind, not Type (FD-043-D33), or None."""
    if name in OUTSIDE_TYPE_NAMES or not TYPE_WORD.search(name):
        return None
    return "the word `type`: it is `kind` (FD-043-D33: `PlayerKind`, `unit_kind`, `KIND_2`)"


def snake_problem(name: str) -> str | None:
    """Why the `snake_case` or `SCREAMING_SNAKE_CASE` `name` breaks C-CASE, or None."""
    words = [w for w in name.split("_") if w]
    if len(words) < 2:
        return None
    if any(len(w) == 1 and w.isalpha() for w in words[:-1]):
        return "a single letter as a word before the last (no article, no possessive)"
    return None


def block_end(code: str, i: int) -> int:
    """The offset of the `}` that closes the block whose `{` is just before `i`."""
    depth = 1
    for j in range(i, len(code)):
        if code[j] == "{":
            depth += 1
        elif code[j] == "}":
            depth -= 1
            if depth == 0:
                return j
    return len(code)


def variants(code: str) -> list[tuple[int, str]]:
    """Each enum variant of `code` with its offset: the names at the enum body's top level."""
    out = []
    for m in ENUM_DECL.finditer(code):
        start, end = m.end(), block_end(code, m.end())
        body = list(code[start:end])
        depth = 0
        for k, c in enumerate(body):  # blank the nested fields and discriminants
            if c in "{(":
                depth += 1
            elif c in "})":
                depth -= 1
                body[k] = " "
                continue
            if depth:
                body[k] = " " if c != "\n" else c
        top = "".join(body)
        top = re.sub(r"=[^,]*", "", top)
        out.extend((start + v.start(1), v.group(1)) for v in VARIANT.finditer(top))
    return out


def enum_names(code: str) -> set[str]:
    return {m.group(1) for m in ENUM_DECL.finditer(code)}


def problems(src: str, enums: set[str]) -> list[tuple[int, str, str]]:
    """Each name of `src` that breaks C-CASE, as (line, name, why); `enums` names the enums of
    the code base (an impl block of one may hold no constant of its own type)."""
    code = mask(src)
    found: list[tuple[int, str, str]] = []
    seen: set[tuple[int, str]] = set()

    def add(offset: int, name: str, why: str | None) -> None:
        line = line_of(src, offset)
        if why and (line, name) not in seen:
            seen.add((line, name))
            found.append((line, name, why))

    for m in TYPE_DECL.finditer(code):
        add(m.start(1), m.group(1), camel_problem(m.group(1)))
    for offset, name in variants(code):
        add(offset, name, camel_problem(name))
    for m in IDENT.finditer(code):
        if "_" in m.group(0):
            add(m.start(), m.group(0), snake_problem(m.group(0)))
        add(m.start(), m.group(0), kind_problem(m.group(0)))
    for m in IMPL.finditer(code):
        name = m.group(1)
        if name not in enums:
            continue
        body_start = m.end()
        body = code[body_start : block_end(code, body_start)]
        for c in re.finditer(SELF_CONST.format(name=re.escape(name)), body):
            add(
                body_start + c.start(1),
                f"{name}::{c.group(1)}",
                "a constant of an enum's own type: an enum's values are its variants",
            )
    return sorted(found)


def crate_of(path: Path) -> Path:
    """The folder of the crate `path` belongs to: the nearest one above it with a `Cargo.toml`."""
    for folder in path.parents:
        if (folder / "Cargo.toml").is_file():
            return folder
    return Path()


def main() -> int:
    files = [f for f in rust_files() if "/target" not in f.as_posix()]
    sources = {f: f.read_text() for f in files}
    enums: dict[Path, set[str]] = {}
    others: dict[Path, set[str]] = {}
    for f, s in sources.items():
        code = mask(s)
        enums.setdefault(crate_of(f), set()).update(enum_names(code))
        others.setdefault(crate_of(f), set()).update(
            m.group(1) for m in TYPE_DECL.finditer(code) if not m.group(0).startswith("enum")
        )
    bad = []
    for f, s in sources.items():
        crate = crate_of(f)
        # a name that is an enum in one module of the crate and a struct in another counts only
        # where the file declares the enum itself
        mine = (enums[crate] - others[crate]) | enum_names(mask(s))
        bad.extend((f, *p) for p in problems(s, mine))
    for f, line, name, why in bad:
        print(f"{f}:{line}: {name}: {why}")
    if bad:
        print(f"c-case: {len(bad)} names break RFC 430 (CODING_STYLE.md section 3)")
        return 1
    print(f"c-case: the names of {len(files)} files follow RFC 430")
    return 0


if __name__ == "__main__":
    sys.exit(main())
