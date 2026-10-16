"""Check only normal direct dependencies using Cargo's resolved package IDs."""
import json
import sys

ALLOWED = {
    "ske-common": set(),
    "ske-cir": {"ske-common"},
    "ske-graph": {"ske-common", "ske-cir"},
    "ske-storage": {"ske-common", "ske-cir"},
    "ske-ingest": {"ske-common", "ske-cir", "ske-storage", "ske-graph"},
    "ske-solver": {"ske-common", "ske-cir"},
    "ske-abstract": {"ske-common", "ske-cir", "ske-graph"},
    "ske-symbolic": {"ske-common", "ske-cir", "ske-graph", "ske-solver", "ske-abstract"},
    "ske-summary": {"ske-common", "ske-cir", "ske-graph", "ske-symbolic", "ske-solver", "ske-abstract"},
    "ske-behavior": {"ske-common", "ske-cir", "ske-graph", "ske-symbolic", "ske-solver", "ske-abstract"},
    "ske-verify": {"ske-common", "ske-cir", "ske-solver", "ske-storage"},
    "ske-query": set(),
    "ske-cli": {"ske-common", "ske-query"},
    "ske-api": {"ske-common", "ske-query"},
    "ske-mcp": {"ske-common", "ske-query"},
}
ALLOWED["ske-query"] = set(ALLOWED) - {"ske-query", "ske-cli", "ske-api", "ske-mcp"}
DENIED_EXTERNAL = {
    "ske-cir": {"rusqlite", "z3"},
    "ske-graph": {"rusqlite", "z3"},
    "ske-ingest": {"z3"},
    "ske-abstract": {"rusqlite", "z3"},
    "ske-cli": {"rusqlite"},
    "ske-api": {"rusqlite"},
    "ske-mcp": {"rusqlite"},
}


def violations(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    workspace = set(metadata["workspace_members"])
    result = []
    for node in metadata["resolve"]["nodes"]:
        if node["id"] not in workspace:
            continue
        name = packages[node["id"]]["name"]
        if name not in ALLOWED:
            result.append(f"FORBIDDEN_DEP unregistered workspace crate {name}")
            continue
        for dep in node["deps"]:
            if not any(k["kind"] is None for k in dep["dep_kinds"]):
                continue
            dependency = packages[dep["pkg"]]["name"]
            if ((dep["pkg"] in workspace and dependency not in ALLOWED[name])
                    or dependency in DENIED_EXTERNAL.get(name, set())):
                result.append(f"FORBIDDEN_DEP {name} -> {dependency}")
    return sorted(set(result))


def selftest():
    # Synthetic metadata tests aliases, dev/build exemption and the exhaustive
    # workspace whitelist without editing anyone's Cargo.toml.
    data = {
        "packages": [{"id": n, "name": n} for n in ("ske-common", "ske-cir", "ske-cli", "rusqlite")],
        "workspace_members": ["ske-common", "ske-cir", "ske-cli"],
        "resolve": {"nodes": [
            {"id": "ske-common", "deps": [
                {"name": "alias", "pkg": "ske-cir", "dep_kinds": [{"kind": None}]}
            ]},
            {"id": "ske-cli", "deps": [
                {"name": "sql", "pkg": "rusqlite", "dep_kinds": [{"kind": "dev"}]},
                {"name": "cir", "pkg": "ske-cir", "dep_kinds": [{"kind": "build"}]}
            ]},
        ]},
    }
    actual = violations(data)
    assert actual == ["FORBIDDEN_DEP ske-common -> ske-cir"], actual
    print(actual[0])
    data["resolve"]["nodes"][1]["deps"][0]["dep_kinds"] = [{"kind": None}]
    assert "FORBIDDEN_DEP ske-cli -> rusqlite" in violations(data)
    print("DEPS SELFTEST OK")


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        selftest()
    else:
        with open(sys.argv[1], encoding="utf-8") as stream:
            errors = violations(json.load(stream))
        if errors:
            print("\n".join(errors))
            sys.exit(1)
        print("DEPS OK")
