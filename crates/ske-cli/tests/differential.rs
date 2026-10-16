mod go_support;
use go_support::*;
use serde_json::Value;
use std::process::Command;

#[test]
fn differential_concrete_grid_is_not_excluded() {
    let out = Command::new("go")
        .args(["run", "."])
        .current_dir(root().join("tests/differential"))
        .env("GOTOOLCHAIN", "local")
        .output()
        .expect("trusted differential runner requires Go");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let observations: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(observations.len(), 40);
    let lab = Lab::new();
    for name in ["Decide", "Check", "AddOne"] {
        for o in observations.iter().filter(|o| o["function"] == name) {
            let x = o["x"].as_i64().unwrap();
            let enabled = o["enabled"].as_bool().unwrap();
            // Constrain the integer input to the actual observation. Boolean
            // context ranges are deliberately forbidden by §11.4.
            let param = if name == "Check" { "age" } else { "x" };
            let context = lab.dir.path().join("context.toml");
            std::fs::write(
                &context,
                format!(
                    "name = \"grid\"\n[[param_range]]\nparam = \"{param}\"\nmin = {x}\nmax = {x}\n"
                ),
            )
            .unwrap();
            lab.ok(&[
                "branches",
                "analyze",
                "--entry",
                &format!("example.{name}"),
                "--context",
                context.to_str().unwrap(),
            ]);
            let branches = lab.ok(&[
                "branches",
                "list",
                "--function",
                &format!("example.{name}"),
                "--run",
                "latest",
            ]);
            let source = branches["source_decisions"].as_array().unwrap();
            let expected = match name {
                "Decide" => {
                    if x < 0 {
                        "-1"
                    } else {
                        "0"
                    }
                }
                "Check" => {
                    if x < 18 {
                        "underage"
                    } else if !enabled {
                        "disabled"
                    } else {
                        "nil"
                    }
                }
                "AddOne" => {
                    if x == 127 {
                        "1"
                    } else {
                        "0"
                    }
                }
                _ => unreachable!(),
            };
            assert_eq!(o["result"], expected);
            let taken: Vec<(usize, bool)> = match name {
                "Decide" => {
                    if x < 0 {
                        vec![(0, true)]
                    } else {
                        vec![(0, false), (1, false)]
                    }
                }
                "Check" => {
                    if x < 18 {
                        vec![(0, true)]
                    } else {
                        vec![(0, false), (1, !enabled)]
                    }
                }
                "AddOne" => vec![(0, x == 127)],
                _ => unreachable!(),
            };
            for (decision, yes) in taken {
                let outcome = &source[decision]["outcomes"][if yes { 0 } else { 1 }];
                assert_ne!(outcome["status"], "PROVEN_UNREACHABLE", "{o}: {outcome}");
                assert_ne!(
                    outcome["status"], "UNANALYZED",
                    "missing modeled observation"
                );
            }
        }
    }
}
