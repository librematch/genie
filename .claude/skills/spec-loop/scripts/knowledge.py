#!/usr/bin/env python3
"""Index spec-loop spec files into .knowledge.db sqlite.

The database lives at the repo root (default ./.knowledge.db). Every spec file
is a Markdown file whose first ```json fenced block is the meta header. Known
fields get columns; the whole header is also stored verbatim.

Usage:
  knowledge.py init [--db PATH]
  knowledge.py index SPEC.md... [--db PATH]      # upsert
  knowledge.py list [--status STATUS] [--db PATH]
  knowledge.py show ID [--db PATH]
  knowledge.py next [--db PATH]                  # oldest open spec
  knowledge.py update ID [--status S] [--outcome T] [--db PATH]
  knowledge.py counts [--db PATH]
"""

import argparse
import json
import pathlib
import re
import sqlite3
import sys
from datetime import datetime, timezone

DEFAULT_DB = ".knowledge.db"

# Known columns; anything else in the JSON header is preserved in `header`.
COLUMNS = [
    "id", "title", "category", "status", "question", "options", "recommended",
    "verify", "blocks", "unblocks", "notes", "outcome", "next",
]

JSON_BLOCK = re.compile(r"```json\s*\n(.*?)\n```", re.DOTALL)
H1 = re.compile(r"^#\s+(.+)$", re.MULTILINE)


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def connect(db: str) -> sqlite3.Connection:
    con = sqlite3.connect(db)
    con.row_factory = sqlite3.Row
    con.execute(
        """CREATE TABLE IF NOT EXISTS specs (
            id TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            title TEXT,
            category TEXT,
            status TEXT NOT NULL DEFAULT 'open',
            question TEXT,
            options TEXT,
            recommended TEXT,
            verify TEXT,
            blocks TEXT,
            unblocks TEXT,
            notes TEXT,
            outcome TEXT,
            next TEXT,
            header TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"""
    )
    con.execute(
        """CREATE TABLE IF NOT EXISTS events (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            spec_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            detail TEXT,
            at TEXT NOT NULL,
            FOREIGN KEY (spec_id) REFERENCES specs(id)
        )"""
    )
    return con


def loads_lenient(text: str) -> dict:
    """json.loads, but raw newlines/tabs inside string literals are escaped
    first — spec headers are hand- or agent-written and wrap prose across
    lines for readability."""
    out = []
    in_string = False
    escaped = False
    for ch in text:
        if in_string and not escaped and ch in "\n\r\t":
            out.append({"\n": "\\n", "\r": "\\r", "\t": "\\t"}[ch])
            continue
        out.append(ch)
        if in_string and ch == "\\" and not escaped:
            escaped = True
            continue
        escaped = False
        if ch == '"':
            in_string = not in_string
    return json.loads("".join(out))


def parse_spec(path: pathlib.Path) -> dict:
    text = path.read_text(encoding="utf-8")
    m = JSON_BLOCK.search(text)
    if not m:
        raise ValueError(f"{path} has no ```json meta header")
    raw = m.group(1)
    try:
        header = json.loads(raw)
    except json.JSONDecodeError:
        try:
            header = loads_lenient(raw)
        except json.JSONDecodeError as e:
            raise ValueError(f"{path} header is not valid JSON: {e}")
    t = H1.search(text)
    title = header.get("title") or (t.group(1).strip() if t else path.stem)
    spec_id = header.get("id") or path.stem
    row = {"id": spec_id, "title": title, "path": str(path), "header": raw}
    for col in COLUMNS:
        if col == "id":
            continue
        v = header.get(col)
        if isinstance(v, (list, dict)):
            v = json.dumps(v, ensure_ascii=False)
        elif v is not None:
            v = str(v)
        row[col] = v
    return row


def upsert(con: sqlite3.Connection, row: dict) -> None:
    existing = con.execute("SELECT created_at FROM specs WHERE id=?", (row["id"],)).fetchone()
    if existing:
        row["created_at"] = existing["created_at"]
        action = "updated"
    else:
        row["created_at"] = now()
        if not row.get("status"):
            row["status"] = "open"
        action = "created"
    row["updated_at"] = now()
    cols = ["id", "path", "title", "category", "status", "question", "options",
            "recommended", "verify", "blocks", "unblocks", "notes", "outcome",
            "next", "header", "created_at", "updated_at"]
    con.execute(
        f"INSERT OR REPLACE INTO specs ({', '.join(cols)}) "
        f"VALUES ({', '.join('?' * len(cols))})",
        [row.get(c) for c in cols],
    )
    con.execute(
        "INSERT INTO events (spec_id, kind, detail, at) VALUES (?, 'indexed', ?, ?)",
        (row["id"], action, now()),
    )


def cmd_index(args: argparse.Namespace) -> None:
    con = connect(args.db)
    indexed = 0
    for f in args.files:
        path = pathlib.Path(f)
        try:
            row = parse_spec(path)
        except ValueError as e:
            print(f"skipped: {e}")
            continue
        upsert(con, row)
        indexed += 1
        print(f"indexed {row['id']} ({row['status']})")
    if indexed == 0:
        sys.exit("error: nothing indexed")
    con.commit()
    con.close()


def cmd_init(args: argparse.Namespace) -> None:
    con = connect(args.db)
    con.commit()
    con.close()
    print(f"initialised {args.db}")


def cmd_list(args: argparse.Namespace) -> None:
    con = connect(args.db)
    sql = "SELECT id, status, category, title FROM specs"
    params: tuple = ()
    if args.status:
        sql += " WHERE status=?"
        params = (args.status,)
    sql += " ORDER BY updated_at"
    for r in con.execute(sql, params):
        print(f"{r['status']:>12}  {r['id']:<40} {r['category'] or '':<10} {r['title'] or ''}")
    con.close()


def cmd_show(args: argparse.Namespace) -> None:
    con = connect(args.db)
    r = con.execute("SELECT * FROM specs WHERE id=?", (args.id,)).fetchone()
    if not r:
        sys.exit(f"error: no spec '{args.id}'")
    for k in r.keys():
        v = r[k]
        if v is not None and len(str(v)) > 200 and k == "header":
            v = str(v) + " …"
        print(f"{k:>12}: {v}")
    print("\nevents:")
    for e in con.execute(
        "SELECT kind, detail, at FROM events WHERE spec_id=? ORDER BY seq", (args.id,)
    ):
        print(f"  {e['at']}  {e['kind']:<10} {e['detail'] or ''}")
    con.close()


def cmd_next(args: argparse.Namespace) -> None:
    con = connect(args.db)
    r = con.execute(
        "SELECT * FROM specs WHERE status IN ('open','in-progress') "
        "ORDER BY updated_at LIMIT 1"
    ).fetchone()
    if not r:
        print("backlog empty: no open or in-progress specs")
    else:
        print(f"next: {r['id']} ({r['status']})")
        if r["question"]:
            print(r["question"])
    con.close()


def cmd_update(args: argparse.Namespace) -> None:
    con = connect(args.db)
    r = con.execute("SELECT id FROM specs WHERE id=?", (args.id,)).fetchone()
    if not r:
        sys.exit(f"error: no spec '{args.id}'")
    if args.status:
        con.execute("UPDATE specs SET status=?, updated_at=? WHERE id=?",
                    (args.status, now(), args.id))
    if args.outcome is not None:
        con.execute("UPDATE specs SET outcome=?, updated_at=? WHERE id=?",
                    (args.outcome, now(), args.id))
    if args.next is not None:
        con.execute("UPDATE specs SET next=?, updated_at=? WHERE id=?",
                    (args.next, now(), args.id))
    con.execute(
        "INSERT INTO events (spec_id, kind, detail, at) VALUES (?, 'updated', ?, ?)",
        (args.id, "; ".join(
            f"{k}={v}" for k, v in
            (("status", args.status), ("outcome", args.outcome), ("next", args.next))
            if v is not None
        ) or "(no change)", now()),
    )
    con.commit()
    con.close()
    print(f"updated {args.id}")


def cmd_counts(args: argparse.Namespace) -> None:
    con = connect(args.db)
    for r in con.execute(
        "SELECT status, COUNT(*) AS n FROM specs GROUP BY status ORDER BY n DESC"
    ):
        print(f"{r['status']:>12}: {r['n']}")
    con.close()


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("init"); s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_init)

    s = sub.add_parser("index"); s.add_argument("files", nargs="+")
    s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_index)

    s = sub.add_parser("list"); s.add_argument("--status")
    s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_list)

    s = sub.add_parser("show"); s.add_argument("id")
    s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_show)

    s = sub.add_parser("next"); s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_next)

    s = sub.add_parser("update"); s.add_argument("id")
    s.add_argument("--status"); s.add_argument("--outcome"); s.add_argument("--next")
    s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_update)

    s = sub.add_parser("counts"); s.add_argument("--db", default=DEFAULT_DB); s.set_defaults(f=cmd_counts)

    args = p.parse_args()
    args.f(args)


if __name__ == "__main__":
    main()
