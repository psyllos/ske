#!/usr/bin/env python3
"""Navigate an SKE SQLite knowledge base without building the Rust CLI.

Read-only, deterministic, stdlib-only (sqlite3). Every subcommand takes the
path to a .sqlite produced by `ske ingest` / `ske init`. Local keys carried
with the `unit_key|` prefix are shown without the prefix (DTO convention).

Subcommands:
  summary      repositories, snapshots, units, and top-level counts
  schema       table names with row counts (schema map)
  functions    functions of a snapshot (or of the CLOSED latest)
  scenarios    reconstructed scenarios + capability + link count
  chain        causal chain of one entry (scenarios show equivalent)
  capabilities capabilities: title, origin, validation status, scenario count
  rules        functional rules (optionally with evidence spans)
  docs         path hint + list of what `ske docs render` would write

Examples:
  python3 ske-navigate.py summary knowledge.sqlite
  python3 ske-navigate.py functions knowledge.sqlite --external
  python3 ske-navigate.py scenarios knowledge.sqlite
  python3 ske-navigate.py chain knowledge.sqlite --entry example.Check --depth 3
  python3 ske-navigate.py rules knowledge.sqlite --with-evidence

The tool NEVER writes to the database and never invents business text.
"""

import argparse
import sqlite3
import sys
from pathlib import Path
from typing import NoReturn


def die(msg: str) -> NoReturn:
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(2)


def open_db(path: str) -> sqlite3.Connection:
    p = Path(path)
    if not p.exists():
        die(f"database not found: {path} (did you run `ske ingest` first?)")
    conn = sqlite3.connect(f"file:{p.resolve()}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def local(value: str) -> str:
    """Strip the `unit_key|` prefix from a local key (DTO convention)."""
    if value is None:
        return ""
    return value.split("|", 1)[-1] if "|" in value else value


def cli_exists() -> bool:
    return Path("target/debug/ske").exists() or shutil_which("ske") is not None


def shutil_which(name: str) -> str | None:
    import shutil

    return shutil.which(name)


def resolve_repo(conn: sqlite3.Connection, repository: str | None) -> int:
    """Mirror Query::snapshot: require --repository when ambiguous."""
    rows = conn.execute(
        "SELECT id, name FROM repositories WHERE (?1 IS NULL OR name=?1 OR CAST(id AS TEXT)=?1) ORDER BY name",
        (repository,),
    ).fetchall()
    if repository is None and len(rows) > 1:
        die(
            "multiple repositories; pass --repository one of: "
            + ", ".join(r["name"] for r in rows)
        )
    if not rows:
        die("no repository found in this database")
    return rows[0]["id"]


def resolve_snapshot(conn: sqlite3.Connection, repo: int, snapshot: str | None) -> int:
    """latest => most recent CLOSED snapshot of the repository."""
    if snapshot and snapshot != "latest":
        row = conn.execute(
            "SELECT id FROM snapshots WHERE id=?1 AND repository_id=?2", (snapshot, repo)
        ).fetchone()
        if row is None:
            die(f"snapshot {snapshot!r} not found for this repository")
        return row["id"]
    row = conn.execute(
        "SELECT id FROM snapshots WHERE repository_id=?1 AND status='CLOSED' ORDER BY id DESC LIMIT 1",
        (repo,),
    ).fetchone()
    if row is None:
        die("no CLOSED snapshot; run `ske ingest` until inventory closes")
    return row["id"]


def snapshot_label(conn: sqlite3.Connection, sid: int) -> str:
    row = conn.execute(
        "SELECT s.id AS sn, r.name AS repo, s.status, s.inventory_status, s.created_at "
        "FROM snapshots s JOIN repositories r ON r.id=s.repository_id WHERE s.id=?1",
        (sid,),
    ).fetchone()
    if row is None:
        return f"#{sid}"
    return (
        f"#{row['sn']} repo={row['repo']!r} status={row['status']} "
        f"inventory={row['inventory_status']} created={row['created_at']}"
    )


def show_snapshot_header(conn, sid):
    print(f"snapshot {snapshot_label(conn, sid)}")
    print()


def cmd_summary(args):
    conn = open_db(args.db)
    print(f"database: {Path(args.db).resolve()}")
    print()
    repos = conn.execute(
        "SELECT id, name FROM repositories ORDER BY name"
    ).fetchall()
    print(f"repositories ({len(repos)})")
    for r in repos:
        snaps = conn.execute(
            "SELECT id, status, inventory_status, created_at FROM snapshots "
            "WHERE repository_id=?1 ORDER BY id DESC",
            (r["id"],),
        ).fetchall()
        print(f"  {r['name']} (id={r['id']}): {len(snaps)} snapshot(s)")
        for s in snaps:
            print(
                f"    #{s['id']} status={s['status']} inventory={s['inventory_status']} "
                f"created={s['created_at']}"
            )
    print()
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    print("current snapshot (--snapshot latest default):")
    show_snapshot_header(conn, sid)
    tree = conn.execute(
        """SELECT u.unit_key, u.language, u.inventory_status,
                  (SELECT COUNT(*) FROM functions f JOIN symbols s ON s.id=f.symbol_id
                    WHERE s.unit_id=u.id AND f.is_external=0) AS functions,
                  (SELECT COUNT(*) FROM basic_blocks b JOIN functions f ON f.id=b.function_id
                    JOIN symbols s ON s.id=f.symbol_id WHERE s.unit_id=u.id) AS blocks
           FROM compilation_units u WHERE u.snapshot_id=?1 ORDER BY u.unit_key""",
        (sid,),
    ).fetchall()
    print(f"compilation units ({len(tree)})")
    for u in tree:
        print(
            f"  {u['unit_key']} :: {u['language']} inventory={u['inventory_status']} "
            f"functions={u['functions']} blocks={u['blocks']}"
        )
    counts = {
        "functions": conn.execute(
            "SELECT COUNT(*) FROM functions f JOIN symbols s ON s.id=f.symbol_id "
            "WHERE s.snapshot_id=?1 AND f.is_external=0",
            (sid,),
        ).fetchone()[0],
        "scenarios": conn.execute(
            "SELECT COUNT(*) FROM behavior_scenarios bs JOIN analysis_runs r ON r.id=bs.analysis_run_id "
            "WHERE r.snapshot_id=?1",
            (sid,),
        ).fetchone()[0],
        "capabilities": conn.execute(
            "SELECT COUNT(*) FROM functional_capabilities WHERE snapshot_id=?1", (sid,)
        ).fetchone()[0],
        "rules": conn.execute(
            "SELECT COUNT(*) FROM functional_rules WHERE snapshot_id=?1", (sid,)
        ).fetchone()[0],
        "scenario_links": conn.execute(
            "SELECT COUNT(*) FROM scenario_links sl JOIN behavior_scenarios bs ON bs.id=sl.from_scenario_id "
            "JOIN analysis_runs r ON r.id=bs.analysis_run_id WHERE r.snapshot_id=?1",
            (sid,),
        ).fetchone()[0],
    }
    print()
    print("top-level counts:")
    for k, v in counts.items():
        print(f"  {k}: {v}")
    if cli_exists():
        print()
        print(
            "tip: build the CLI to get richer DTO views"
            " (`ske scenarios show --entry E`)"
        )
    conn.close()


def cmd_schema(args):
    conn = open_db(args.db)
    tables = conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    ).fetchall()
    print(f"tables ({len(tables)})")
    for (name,) in tables:
        cnt = conn.execute(f"SELECT COUNT(*) FROM \"{name}\"").fetchone()[0]
        print(f"  {name}: {cnt}")
    conn.close()


def cmd_functions(args):
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    show_snapshot_header(conn, sid)
    sql = (
        "SELECT s.qualified_name, u.unit_key, f.is_external, f.inventory_status, "
        "(SELECT COUNT(*) FROM basic_blocks WHERE function_id=f.id) AS blocks "
        "FROM functions f JOIN symbols s ON s.id=f.symbol_id "
        "JOIN compilation_units u ON u.id=s.unit_id "
        "WHERE s.snapshot_id=?1 AND (?2=1 OR f.is_external=0) "
        "ORDER BY s.qualified_name, u.unit_key"
    )
    rows = conn.execute(sql, (sid, 1 if args.external else 0)).fetchall()
    print(f"functions ({len(rows)})")
    for r in rows:
        ext = " external" if r["is_external"] else ""
        print(
            f"  {r['qualified_name']} :: {local(r['unit_key'])}{ext} "
            f"inventory={r['inventory_status']} blocks={r['blocks']}"
        )
    conn.close()


def cmd_scenarios(args):
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    show_snapshot_header(conn, sid)
    rows = conn.execute(
        """SELECT bs.id, bs.name, bs.status, bs.precision, fc.title AS cap_title,
                  fc.validation_status AS cap_status,
                  (SELECT COUNT(*) FROM scenario_links sl WHERE sl.from_scenario_id=bs.id) AS link_count
           FROM behavior_scenarios bs
           JOIN analysis_runs r ON r.id=bs.analysis_run_id
           LEFT JOIN capability_scenarios csc ON csc.scenario_id=bs.id
           LEFT JOIN functional_capabilities fc ON fc.id=csc.capability_id
           WHERE r.snapshot_id=?1
           ORDER BY bs.name, bs.id""",
        (sid,),
    ).fetchall()
    print(f"scenarios ({len(rows)})")
    for r in rows:
        cap = r["cap_title"] or "(non nommée)"
        print(
            f"  {r['name']} status={r['status']} precision={r['precision']} "
            f"capability={cap!r} ({r['cap_status']}) links={r['link_count']}"
        )
    conn.close()


def cmd_chain(args):
    """Equivalent of `ske scenarios show --entry E --depth N` via SQL."""
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)

    def function_row(qualified: str):
        return conn.execute(
            "SELECT f.id, s.qualified_name AS name, u.unit_key FROM functions f "
            "JOIN symbols s ON s.id=f.symbol_id JOIN compilation_units u ON u.id=s.unit_id "
            "WHERE s.snapshot_id=?1 AND s.qualified_name=?2 AND f.is_external=0 "
            "ORDER BY u.unit_key LIMIT 1",
            (sid, qualified),
        ).fetchone()

    def scenario_of(fid: int):
        return conn.execute(
            "SELECT id, name, status, precision FROM behavior_scenarios "
            "WHERE entry_function_id=?1 ORDER BY id DESC LIMIT 1",
            (fid,),
        ).fetchone()

    def cases(sid_scenario: int):
        return conn.execute(
            "SELECT case_ordinal, guard_text, result_text, precision FROM behavior_cases "
            "WHERE behavior_id=?1 ORDER BY case_ordinal",
            (sid_scenario,),
        ).fetchall()

    def callees(fid: int):
        return conn.execute(
            "SELECT ct.target_function_id, t_s.qualified_name FROM call_sites cs "
            "JOIN instructions i ON i.id=cs.instruction_id "
            "JOIN basic_blocks b ON b.id=i.block_id "
            "JOIN call_targets ct ON ct.call_site_id=cs.id "
            "JOIN functions t ON t.id=ct.target_function_id "
            "JOIN symbols t_s ON t_s.id=t.symbol_id "
            "WHERE b.function_id=?1 AND cs.dispatch_kind='STATIC' "
            "AND cs.world_closed=1 AND cs.resolution_status='RESOLVED_UNIQUE' AND t.is_external=0 "
            "ORDER BY t_s.qualified_name",
            (fid,),
        ).fetchall()

    root = function_row(args.entry)
    if root is None:
        die(f"entry {args.entry!r} not found in snapshot")

    def walk(fid: int, name: str, depth: int, indent: str):
        sc = scenario_of(fid)
        if sc is None:
            print(f"{indent}{name}: NOT_RECONSTRUCTED")
            return
        print(f"{indent}{name}: scenario #{sc['id']} status={sc['status']} precision={sc['precision']}")
        for c in cases(sc["id"]):
            print(f"{indent}  case {c['case_ordinal']}: IF {c['guard_text']!r} => {c['result_text']!r}")
        if depth > 0:
            for cal in callees(fid):
                walk(cal["target_function_id"], cal["qualified_name"], depth - 1, indent + "  ")

    walk(root["id"], root["name"], args.depth, "")
    conn.close()


def cmd_capabilities(args):
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    show_snapshot_header(conn, sid)
    rows = conn.execute(
        """SELECT fc.canonical_key, fc.title, fc.origin, fc.validation_status,
                  (SELECT COUNT(*) FROM capability_scenarios csc WHERE csc.capability_id=fc.id) AS scenario_count
           FROM functional_capabilities fc
           WHERE fc.snapshot_id=?1
           ORDER BY fc.canonical_key""",
        (sid,),
    ).fetchall()
    print(f"capabilities ({len(rows)})")
    for r in rows:
        title = r["title"] or "(non nommée)"
        print(
            f"  {r['canonical_key']} title={title!r} origin={r['origin']} "
            f"status={r['validation_status']} scenarios={r['scenario_count']}"
        )
    conn.close()


def cmd_rules(args):
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    show_snapshot_header(conn, sid)
    sql = (
        "SELECT fr.id, fr.rule_key, fr.short_name, fr.description, fr.origin, "
        "fr.validation_status, bc.guard_text, bc.result_text, "
        "sc.exit_span_id AS span "
        "FROM functional_rules fr "
        "LEFT JOIN behavior_cases bc ON bc.id=fr.behavior_case_id "
        "LEFT JOIN summary_cases sc ON sc.id=bc.summary_case_id "
        "WHERE fr.snapshot_id=?1 ORDER BY fr.id"
    )
    rows = conn.execute(sql, (sid,)).fetchall()
    print(f"rules ({len(rows)})")
    for r in rows:
        line = (
            f"  {r['rule_key']} [{r['origin']}/{r['validation_status']}] "
            f"{r['description']!r}"
        )
        if args.with_evidence and r["guard_text"]:
            line += f"  (IF {r['guard_text']!r} => {r['result_text']!r})"
        if args.with_evidence and r["span"]:
            line += f"  span_id={r['span']}"
        print(line)
    conn.close()


def cmd_docs(args):
    conn = open_db(args.db)
    repo = resolve_repo(conn, args.repository)
    sid = resolve_snapshot(conn, repo, args.snapshot)
    print("docs render is a Rust CLI feature; this script prints the expected output set.")
    print()
    show_snapshot_header(conn, sid)
    print("`ske docs render --snapshot latest --out DIR` writes:")
    print("  INDEX.md            index + 'how to read this documentation' legend")
    print("  <function>.md        one fiche per reconstructed function")
    print("  (+ CAPABILITIES.md  with --capabilities: top-down business view)")
    print()
    rows = conn.execute(
        "SELECT DISTINCT bs.name FROM behavior_scenarios bs "
        "JOIN analysis_runs r ON r.id=bs.analysis_run_id WHERE r.snapshot_id=?1 "
        "ORDER BY bs.name",
        (sid,),
    ).fetchall()
    for r in rows:
        print(f"  would match fiche: {r['name']}")
    if not cli_exists():
        print()
        print(
            "tip: build the CLI (`cargo build -p ske-cli`) to actually write the fiches."
        )
    conn.close()


def main() -> None:
    p = argparse.ArgumentParser(prog="ske-navigate", description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name, help_txt in [
        ("summary", "overview of the database"),
        ("schema", "tables with row counts"),
        ("functions", "functions of the current snapshot"),
        ("scenarios", "reconstructed scenarios"),
        ("chain", "causal chain of one entry"),
        ("capabilities", "capabilities (titles/status)"),
        ("rules", "functional rules"),
        ("docs", "what docs render would produce"),
    ]:
        sp = sub.add_parser(name, help=help_txt)
        sp.set_defaults(fn=getattr(sys.modules[__name__], f"cmd_{name}"))
        sp.add_argument("db", help="path to the SKE SQLite knowledge base")
        sp.add_argument("--snapshot", default="latest", help="snapshot id or 'latest'")
        sp.add_argument("--repository", default=None, help="repository name or id")
        if name == "functions":
            sp.add_argument("--external", action="store_true")
        if name == "chain":
            sp.add_argument("--entry", required=True)
            sp.add_argument("--depth", type=int, default=3)
        if name == "rules":
            sp.add_argument("--with-evidence", action="store_true")
    args = p.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
