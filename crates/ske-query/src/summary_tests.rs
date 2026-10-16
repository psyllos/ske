//! Integration evidence using imported Go bodies, not fixture-specific engine rules.
use crate::analysis_load;
use ske_storage::Storage;
use ske_symbolic::{
    engine::{self, Budget, Soundness},
    summary::{FunctionSummary, Precision},
    BinaryOp, Constant, ExprKind,
};
use std::{collections::BTreeMap, path::PathBuf, process::Command};

fn imported() -> (tempfile::TempDir, Storage) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let adapter = temp.path().join("ske-adapter-go");
    let output = Command::new("go")
        .current_dir(root.join("adapters/go"))
        .args(["build", "-p=1", "-o"])
        .arg(&adapter)
        .arg("./cmd/ske-adapter-go")
        .env("GOTOOLCHAIN", "local")
        .env("GOMAXPROCS", "2")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let storage = Storage::open(temp.path().join("summary.sqlite")).unwrap();
    let profile =
        std::fs::read_to_string(root.join("examples/go-branch-lab/build-profile.json")).unwrap();
    let result = ske_ingest::ingest_go(
        &storage,
        &ske_ingest::GoOptions {
            repo: &root.join("examples/go-branch-lab"),
            adapter: &adapter,
            profile_json: &profile,
            limits: &Default::default(),
            repository: None,
            redact: false,
        },
    )
    .unwrap();
    assert_eq!(result["status"], "CLOSED");
    assert_eq!(result["diagnostics"]["ERROR"], 0);
    (temp, storage)
}

fn function(storage: &Storage, name: &str) -> i64 {
    storage.conn.query_row(
        "SELECT f.id FROM functions f JOIN symbols s ON s.id=f.symbol_id WHERE s.qualified_name=?1 AND f.is_external=0",
        [name], |r| r.get(0)).unwrap()
}

#[test]
fn summary_entry() {
    let (_temp, storage) = imported();
    let helper_id = function(&storage, "example.helper");
    let entry_id = function(&storage, "example.Entry");
    let helper = analysis_load::load(&storage.conn, helper_id).unwrap();
    let entry = analysis_load::load(&storage.conn, entry_id).unwrap();
    let mut solver = ske_solver::Z3Solver::new();
    let report = engine::analyze(
        &helper,
        &Budget::default(),
        &mut solver,
        |_, _| Ok(vec![]),
        &|| false,
        &mut |_| {},
    )
    .unwrap();
    let helper_summary = FunctionSummary::from_report(&helper, report);
    assert_eq!(helper_summary.precision, Precision::Exact);
    assert_eq!(helper_summary.cases.len(), 2);
    let helper_paths: Vec<_> = helper_summary
        .cases
        .iter()
        .map(|e| e.taken_outcomes.ordered())
        .collect();
    // Resolution is taken from persisted call_targets and closed-world claims.
    let (callsite, target, closed): (String, i64, bool) = storage
        .conn
        .query_row(
            "SELECT i.local_key, ct.target_function_id, cs.world_closed FROM call_sites cs
         JOIN instructions i ON i.id=cs.instruction_id JOIN basic_blocks b ON b.id=i.block_id
         JOIN call_targets ct ON ct.call_site_id=cs.id
         WHERE b.function_id=?1 AND cs.resolution_status='RESOLVED_UNIQUE'",
            [entry_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(target, helper_id);
    assert!(closed);
    let prefix = storage
        .conn
        .query_row(
            "SELECT u.unit_key FROM compilation_units u JOIN symbols s ON s.unit_id=u.id
         JOIN functions f ON f.symbol_id=s.id WHERE f.id=?1",
            [entry_id],
            |r| r.get::<_, String>(0),
        )
        .unwrap();
    let callsite = callsite
        .strip_prefix(&format!("{prefix}|"))
        .unwrap()
        .to_owned();
    let summaries = BTreeMap::from([(callsite, helper_summary)]);
    let report = engine::analyze_with_summaries(
        &entry,
        &Budget::default(),
        &mut solver,
        |_, _| Ok(vec![]),
        &|| false,
        &mut |_| {},
        Some(&summaries),
    )
    .unwrap();
    assert!(report.exploration_closed);
    assert_eq!(report.exits.len(), 2);
    for (ordinal, exit) in report.exits.iter().enumerate() {
        assert_eq!(exit.soundness, Soundness::Exact);
        assert_eq!(exit.kind, "RETURN");
        assert_eq!(exit.values.len(), 1);
        assert_eq!(exit.taken_outcomes.ordered(), helper_paths[ordinal]);
        assert_eq!(
            report.arena.get(exit.values[0]).unwrap().kind,
            ExprKind::Const(Constant::Bits(if ordinal == 0 { 1 } else { 0 }))
        );
    }
    let guard = report.arena.get(report.exits[0].constraints[0]).unwrap();
    let ExprKind::Binary {
        op: BinaryOp::Gt,
        left,
        right,
    } = guard.kind
    else {
        panic!("Entry guard must preserve helper's x > 100");
    };
    assert_eq!(
        report.arena.get(left).unwrap().kind,
        ExprKind::Symbol("param:example.Entry:0".into())
    );
    assert_eq!(
        report.arena.get(right).unwrap().kind,
        ExprKind::Const(Constant::Bits(100))
    );
    let summary = FunctionSummary::from_report(&entry, report);
    assert_eq!(summary.precision, Precision::Exact);
    assert!(summary.complete);
    // Apply the same summary under a caller constraint. The true case must
    // be discarded by the solver, not emitted unconditionally.
    let constrained = engine::analyze_with_summaries(
        &entry,
        &Budget::default(),
        &mut solver,
        |arena, env| {
            let parameter = env[&entry.function.parameter_value_ids[0]];
            let ty = arena.get(parameter).unwrap().ty.clone();
            let bound = arena.constant(ty, Constant::Bits(100)).unwrap();
            let guard = arena
                .binary(
                    ske_symbolic::Type {
                        local_key: "engine:bool".into(),
                        sort: ske_symbolic::Sort::Bool,
                    },
                    BinaryOp::Le,
                    parameter,
                    bound,
                )
                .unwrap();
            Ok(vec![guard])
        },
        &|| false,
        &mut |_| {},
        Some(&summaries),
    )
    .unwrap();
    assert_eq!(constrained.exits.len(), 1);
    assert_eq!(
        constrained
            .arena
            .get(constrained.exits[0].values[0])
            .unwrap()
            .kind,
        ExprKind::Const(Constant::Bits(0))
    );
    // UNKNOWN must retain both cases but must not claim an exact closed run.
    let mut unknown = ske_solver::ScriptedSolver::new([
        ske_solver::CheckResult::Unknown {
            reason: "test unknown".into(),
        },
        ske_solver::CheckResult::Unknown {
            reason: "test unknown".into(),
        },
    ]);
    let uncertain = engine::analyze_with_summaries(
        &entry,
        &Budget::default(),
        &mut unknown,
        |_, _| Ok(vec![]),
        &|| false,
        &mut |_| {},
        Some(&summaries),
    )
    .unwrap();
    assert_eq!(uncertain.exits.len(), 2);
    assert!(!uncertain.exploration_closed);
    assert!(uncertain
        .exits
        .iter()
        .all(|e| e.soundness == Soundness::Opaque));
    assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
}

#[test]
fn summary_calldoer() {
    let (_temp, storage) = imported();
    let program =
        analysis_load::load(&storage.conn, function(&storage, "example.CallDoer")).unwrap();
    let report = engine::analyze_with_summaries(
        &program,
        &Budget::default(),
        &mut ske_solver::Z3Solver::new(),
        |_, _| Ok(vec![]),
        &|| false,
        &mut |_| {},
        Some(&BTreeMap::new()),
    )
    .unwrap();
    assert_eq!(report.outcomes.len(), 2);
    assert!(report
        .outcomes
        .values()
        .all(|r| r.status == engine::Classification::Possible && r.soundness == Soundness::Opaque));
    let summary = FunctionSummary::from_report(&program, report);
    assert_eq!(summary.precision, Precision::Overapprox);
    assert_eq!(summary.cases.len(), 2);
    assert!(summary.unknowns.contains(&"CALL_TARGET_UNRESOLVED".into()));
    assert!(!summary.composable);
    let stored = crate::summaries::run(
        &storage.conn,
        1,
        function(&storage, "example.CallDoer"),
        &Budget::default(),
    )
    .unwrap();
    assert_eq!(stored["precision"], "OVERAPPROX");
    let effects: i64 = storage.conn.query_row(
        "SELECT COUNT(*) FROM observable_effects e JOIN summary_cases c ON c.id=e.summary_case_id
         WHERE c.summary_id=?1 AND e.effect_kind='UNKNOWN_EFFECT'",
        [stored["summary_id"].as_i64().unwrap()], |r| r.get(0)).unwrap();
    assert_eq!(effects, 2);
}

#[test]
fn summary_deterministic_and_persisted() {
    let (_temp, storage) = imported();
    let entry = function(&storage, "example.Entry");
    let first = crate::summaries::run(&storage.conn, 1, entry, &Budget::default()).unwrap();
    let second = crate::summaries::run(&storage.conn, 1, entry, &Budget::default()).unwrap();
    assert_eq!(first["summary_fingerprint"], second["summary_fingerprint"]);
    assert_eq!(first["summary_id"], second["summary_id"]);
    assert_eq!(first["precision"], "EXACT");
    assert_eq!(first["cases"], 2);
    let id = first["summary_id"].as_i64().unwrap();
    let count: i64 = storage.conn.query_row(
        "SELECT COUNT(*) FROM summary_case_returns r JOIN summary_cases c ON c.id=r.summary_case_id
         WHERE c.summary_id=?1", [id], |r| r.get(0)).unwrap();
    assert_eq!(count, 2);
    let outcomes: i64 = storage.conn.query_row(
        "SELECT COUNT(*) FROM summary_case_outcomes o JOIN summary_cases c ON c.id=o.summary_case_id
         WHERE c.summary_id=?1", [id], |r| r.get(0)).unwrap();
    assert_eq!(outcomes, 2);
    assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
}

#[test]
fn summary_cut_rolls_back() {
    let (_temp, storage) = imported();
    let memory = function(&storage, "example.DivMod");
    let budget = Budget {
        max_states: 0,
        ..Default::default()
    };
    assert!(crate::summaries::run(&storage.conn, 1, memory, &budget).is_err());
    for table in [
        "function_summaries",
        "summary_cases",
        "summary_case_returns",
        "analysis_runs",
        "symbolic_expressions",
    ] {
        let count: i64 = storage
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "failed summary left rows in {table}");
    }
}

#[test]
fn summary_fact() {
    let (_temp, storage) = imported();
    let fact = function(&storage, "example.Fact");
    let result = crate::summaries::run(&storage.conn, 1, fact, &Budget::default()).unwrap();
    assert_eq!(result["precision"], "PARTIAL");
    assert_eq!(result["complete"], false);
    assert_eq!(result["cases"], 4);
    let id = result["summary_id"].as_i64().unwrap();
    let (complete, unknowns): (bool, String) = storage
        .conn
        .query_row(
            "SELECT complete,unknowns_json FROM function_summaries WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert!(!complete);
    assert!(unknowns.contains("RECURSION_BOUND"));
    let partial: i64 = storage
        .conn
        .query_row(
            "SELECT COUNT(*) FROM summary_cases WHERE summary_id=?1 AND precision='PARTIAL'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(partial, 1);
    let guards: Vec<String> = {
        let mut statement = storage
            .conn
            .prepare(
                "SELECT guard_json FROM summary_cases WHERE summary_id=?1 ORDER BY case_ordinal",
            )
            .unwrap();
        statement
            .query_map([id], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    };
    let lengths: Vec<_> = guards
        .iter()
        .map(|g| serde_json::from_str::<Vec<i64>>(g).unwrap().len())
        .collect();
    assert_eq!(lengths, vec![1, 2, 3, 3], "ordered recursion guards");
    let multiplications: i64 = storage.conn.query_row(
        "WITH RECURSIVE tree(id) AS (
           SELECT r.expression_id FROM summary_case_returns r
           JOIN summary_cases c ON c.id=r.summary_case_id
           WHERE c.summary_id=?1 AND c.precision='PARTIAL'
           UNION ALL
           SELECT o.operand_expression_id FROM expression_operands o JOIN tree t ON t.id=o.expression_id
         ) SELECT COUNT(*) FROM tree t JOIN symbolic_expressions e ON e.id=t.id
         WHERE e.kind='BINARY' AND CAST(e.payload AS TEXT)='\"Mul\"'",
        [id], |r| r.get(0)).unwrap();
    assert_eq!(
        multiplications, 2,
        "residual must be n * ((n-1) * H), not an extra recursive multiplication"
    );
    assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
}

#[test]
fn summary_divmod_and_divide2_multiple_returns() {
    let (_temp, storage) = imported();
    for (name, expected_cases) in [("example.DivMod", 2), ("example.Divide2", 3)] {
        let id = function(&storage, name);
        let result = crate::summaries::run(&storage.conn, 1, id, &Budget::default()).unwrap();
        assert_eq!(result["cases"], expected_cases, "{name}");
        assert_eq!(result["precision"], "EXACT", "{name}");
        let summary_id = result["summary_id"].as_i64().unwrap();
        let returns: i64 = storage.conn.query_row(
            "SELECT COUNT(*) FROM summary_case_returns r JOIN summary_cases c ON c.id=r.summary_case_id
             WHERE c.summary_id=?1", [summary_id], |r| r.get(0)).unwrap();
        assert_eq!(returns, 2 * expected_cases, "{name}");
    }
    assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
}

#[test]
fn summary_global_write_is_propagated_between_calls() {
    let (_temp, storage) = imported();
    let function = function(&storage, "example.ReadChangingGlobal");
    let result = crate::summaries::run(&storage.conn, 1, function, &Budget::default()).unwrap();
    assert_eq!(result["precision"], "EXACT");
    assert_eq!(result["cases"], 1);
    let id = result["summary_id"].as_i64().unwrap();
    let (delta, second): (String, i64) = storage
        .conn
        .query_row(
            "SELECT c.memory_delta_json,r.expression_id FROM summary_cases c
         JOIN summary_case_returns r ON r.summary_case_id=c.id AND r.return_ordinal=1
         WHERE c.summary_id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let delta: serde_json::Value = serde_json::from_str(&delta).unwrap();
    assert_eq!(delta.as_array().unwrap().len(), 1);
    assert_eq!(delta[0]["location"]["symbol"], "example.MutableTotal");
    assert_eq!(delta[0]["expression_id"], second);
    assert!(storage.db_check().unwrap()["ok"].as_bool().unwrap());
}
