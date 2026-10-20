use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use ske_common::{Error, Result};
use ske_query::Query;
use std::path::PathBuf;
mod consent;

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum Format {
    Text,
    Json,
    Yaml,
}

#[derive(Parser)]
#[command(name = "ske", version, about = "Structural Knowledge Engine")]
struct Cli {
    #[arg(long, env = "SKE_DB", global = true)]
    db: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "text", global = true)]
    format: Format,
    #[arg(long, default_value = "latest", global = true)]
    snapshot: String,
    #[arg(long, global = true)]
    repository: Option<String>,
    #[arg(long, default_value_t = 100, global = true)]
    limit: usize,
    #[arg(long, default_value_t = 0, global = true)]
    offset: usize,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Ingest {
        #[command(subcommand)]
        command: Ingest,
    },
    Snapshots {
        #[command(subcommand)]
        command: Snapshots,
    },
    Inspect,
    Functions {
        #[command(subcommand)]
        command: Functions,
    },
    Branches {
        #[command(subcommand)]
        command: Branches,
    },
    Runs {
        #[command(subcommand)]
        command: Runs,
    },
    Db {
        #[command(subcommand)]
        command: Db,
    },
    Paths {
        #[command(subcommand)]
        command: Paths,
    },
    Scenarios {
        #[command(subcommand)]
        command: Scenarios,
    },
    Capabilities {
        #[command(subcommand)]
        command: Capabilities,
    },
    Docs {
        #[command(subcommand)]
        command: Docs,
    },
    Behaviors {
        #[command(subcommand)]
        command: Behaviors,
    },
    Rules {
        #[command(subcommand)]
        command: Rules,
    },
}

#[derive(Subcommand)]
enum Ingest {
    Go(AdapterIngest),
    Rust(AdapterIngest),
    Java(AdapterIngest),
    Cir {
        #[arg(long, required = true)]
        input: Vec<PathBuf>,
        #[arg(long)]
        source_root: Option<PathBuf>,
        #[arg(long)]
        require_complete: bool,
    },
}

#[derive(Args)]
struct AdapterIngest {
    #[arg(long)]
    repo: PathBuf,
    #[arg(long)]
    profile: Option<PathBuf>,
    #[arg(long)]
    adapter: Option<PathBuf>,
    #[arg(long)]
    yes_run_adapter: bool,
    #[arg(long, default_value_t = 16 * 1024 * 1024)]
    max_message_bytes: u64,
    #[arg(long, default_value_t = 50_000_000)]
    max_messages: u64,
    #[arg(long, default_value_t = 600_000)]
    timeout_ms: u64,
    #[arg(long, default_value_t = 4096)]
    max_memory_mb: u64,
    #[arg(long)]
    redact: bool,
    #[arg(long)]
    require_complete: bool,
}
#[derive(Subcommand)]
enum Snapshots {
    List,
}
#[derive(Subcommand)]
enum Functions {
    List {
        #[arg(long)]
        include_external: bool,
    },
    Cfg {
        #[arg(long)]
        function: String,
    },
}
#[derive(Subcommand)]
enum Branches {
    List {
        #[arg(long)]
        function: String,
        #[arg(long)]
        run: Option<String>,
    },
    Analyze {
        #[arg(long)]
        entry: String,
        #[arg(long)]
        budget: Option<PathBuf>,
        #[arg(long)]
        context: Option<PathBuf>,
        #[arg(long)]
        require_complete: bool,
    },
}
#[derive(Subcommand)]
enum Runs {
    List,
    MarkFailed {
        #[arg(long)]
        run: i64,
    },
}
#[derive(Subcommand)]
enum Db {
    Check,
}
#[derive(Subcommand)]
enum Paths {
    Explore {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        max_depth: Option<usize>,
        #[arg(long)]
        max_paths: Option<usize>,
        #[arg(long)]
        max_nodes: Option<usize>,
    },
}
#[derive(Subcommand)]
enum Scenarios {
    List,
    Link {
        #[arg(long)]
        entry: String,
    },
    LinkInter {
        #[arg(long)]
        entry: String,
        #[arg(long)]
        contracts: std::path::PathBuf,
    },
    Show {
        #[arg(long)]
        entry: String,
        #[arg(long, default_value_t = 3)]
        depth: usize,
    },
}
#[derive(Subcommand)]
enum Capabilities {
    List,
    Nominate {
        #[arg(long)]
        entry: String,
        #[arg(long)]
        title: String,
    },
    Reject {
        #[arg(long)]
        entry: String,
    },
    Show {
        #[arg(long)]
        entry: String,
        #[arg(long, default_value_t = 3)]
        depth: usize,
    },
    StaleCheck {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
}
#[derive(Subcommand)]
enum Docs {
    Render {
        #[arg(long)]
        out: std::path::PathBuf,
        #[arg(long)]
        capabilities: bool,
    },
}
#[derive(Subcommand)]
enum Behaviors {
    Reconstruct {
        #[arg(long)]
        entry: String,
    },
}
#[derive(Subcommand)]
enum Rules {
    List {
        #[arg(long)]
        with_evidence: bool,
    },
    StaleCheck {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
}

fn execute(cli: &Cli) -> Result<(Value, i32)> {
    let path = cli
        .db
        .as_ref()
        .ok_or_else(|| Error::new("E_USAGE", "--db PATH or SKE_DB is required"))?;
    if cli.limit > 10_000 {
        return Err(Error::new("E_USAGE", "--limit must not exceed 10000"));
    }
    let mut query = Query::open(path)?;
    let repository = cli.repository.as_deref();
    let mut status = 0;
    let value = match &cli.command {
        Command::Init => query.init()?,
        Command::Ingest {
            command: command @ (Ingest::Go(_) | Ingest::Rust(_) | Ingest::Java(_)),
        } => {
            let (language, args) = match command {
                Ingest::Go(args) => (ske_query::Language::Go, args),
                Ingest::Rust(args) => (ske_query::Language::Rust, args),
                Ingest::Java(args) => (ske_query::Language::Java, args),
                Ingest::Cir { .. } => unreachable!(),
            };
            let AdapterIngest {
                repo,
                profile,
                adapter,
                yes_run_adapter,
                max_message_bytes,
                max_messages,
                timeout_ms,
                max_memory_mb,
                redact,
                require_complete,
            } = args;
            let default_adapter = PathBuf::from(match language {
                ske_query::Language::Go => "target/ske-adapter-go",
                ske_query::Language::Rust => "adapters/rust/target/debug/ske-adapter-rust",
                ske_query::Language::Java => "adapters/java/ske-adapter-java",
            });
            let adapter = adapter.as_ref().unwrap_or(&default_adapter);
            let default_profile = repo.join("build-profile.json");
            let selected_profile = profile
                .as_ref()
                .or_else(|| default_profile.exists().then_some(&default_profile));
            let input = selected_profile
                .map(|path| ske_query::profile::read_profile(path))
                .transpose()?;
            let profile = ske_query::profile::normalize_for(language, input.as_deref())?;
            let limits = ske_query::RunLimits {
                max_message_bytes: *max_message_bytes,
                max_messages: *max_messages,
                timeout_ms: *timeout_ms,
                max_memory_mb: *max_memory_mb,
            };
            limits.validate()?;
            consent::ensure(path, repo, adapter, *yes_run_adapter)?;
            let value = query.ingest_language(
                language,
                &ske_query::IngestOptions {
                    repo,
                    adapter,
                    profile_json: &profile,
                    limits: &limits,
                    repository,
                    redact: *redact,
                },
            )?;
            if value["status"] == "ABORTED" {
                status = 3;
            } else if *require_complete && value["inventory_status"] != "COMPLETE" {
                status = 4;
            }
            value
        }
        Command::Ingest {
            command:
                Ingest::Cir {
                    input,
                    source_root,
                    require_complete,
                },
        } => {
            let value = query.ingest_many(input, source_root.as_deref(), repository)?;
            if value["status"] == "ABORTED" {
                status = 3;
            } else if *require_complete && value["inventory_status"] != "COMPLETE" {
                status = 4;
            }
            value
        }
        Command::Snapshots { .. } => query.snapshots_page(repository, cli.limit, cli.offset)?,
        Command::Inspect => query.inspect(&cli.snapshot, repository)?,
        Command::Functions {
            command: Functions::List { include_external },
        } => query.functions(
            &cli.snapshot,
            repository,
            *include_external,
            cli.limit,
            cli.offset,
        )?,
        Command::Functions {
            command: Functions::Cfg { function },
        } => query.cfg(&cli.snapshot, repository, function)?,
        Command::Branches {
            command: Branches::List { function, run },
        } => match run {
            Some(run) => query.branches_run(
                &cli.snapshot,
                repository,
                function,
                run,
                cli.limit,
                cli.offset,
            )?,
            None => {
                query.branches_page(&cli.snapshot, repository, function, cli.limit, cli.offset)?
            }
        },
        Command::Branches {
            command:
                Branches::Analyze {
                    entry,
                    budget,
                    context,
                    require_complete,
                },
        } => {
            let budget = ske_query::analysis_config::Budget::read(budget.as_deref())?;
            let context = ske_query::analysis_config::Context::read(context.as_deref())?;
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut signals = Vec::new();
            for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
                match signal_hook::flag::register(signal, cancel.clone()) {
                    Ok(id) => signals.push(id),
                    Err(error) => {
                        for id in signals {
                            signal_hook::low_level::unregister(id);
                        }
                        return Err(Error::new("E_INTERNAL", error.to_string()));
                    }
                }
            }
            let result =
                query.analyze(&cli.snapshot, repository, entry, &budget, &context, &cancel);
            for id in signals {
                signal_hook::low_level::unregister(id);
            }
            let value = result?;
            status = if cancel.load(std::sync::atomic::Ordering::Relaxed)
                || value["run_status"] == "CANCELLED"
            {
                130
            } else if value["run_status"] == "FAILED" {
                5
            } else if *require_complete && value["run_status"] != "COMPLETED" {
                4
            } else {
                0
            };
            value
        }
        Command::Runs {
            command: Runs::List,
        } => query.runs(&cli.snapshot, repository, cli.limit, cli.offset)?,
        Command::Runs {
            command: Runs::MarkFailed { run },
        } => query.mark_run_failed(*run)?,
        Command::Db { .. } => {
            let value = query.db_check()?;
            if value["ok"] == false {
                status = 6;
            }
            value
        }
        Command::Paths {
            command:
                Paths::Explore {
                    source,
                    target,
                    max_depth,
                    max_paths,
                    max_nodes,
                },
        } => query.paths_explore(
            &cli.snapshot,
            repository,
            source,
            target,
            *max_depth,
            *max_paths,
            *max_nodes,
            cli.limit,
            cli.offset,
        )?,
        Command::Scenarios {
            command: Scenarios::List,
        } => query.scenarios(&cli.snapshot, repository)?,
        Command::Scenarios {
            command: Scenarios::Link { entry },
        } => query.scenarios_link(&cli.snapshot, repository, entry)?,
        Command::Scenarios {
            command: Scenarios::LinkInter { entry, contracts },
        } => {
            let text = std::fs::read_to_string(contracts).map_err(|e| {
                ske_common::Error::new("E_USAGE", format!("cannot read contracts file: {e}"))
            })?;
            let payload: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                ske_common::Error::new("E_USAGE", format!("invalid contracts JSON: {e}"))
            })?;
            let list = payload["contracts"].clone();
            let contracts: Vec<ske_query::scenarios::InterContract> = serde_json::from_value(list)
                .map_err(|e| {
                    ske_common::Error::new(
                        "E_USAGE",
                        format!("invalid contracts schema (contracts[]): {e}"),
                    )
                })?;
            query.scenarios_link_inter(&cli.snapshot, repository, entry, contracts)?
        }
        Command::Scenarios {
            command: Scenarios::Show { entry, depth },
        } => query.scenarios_show(&cli.snapshot, repository, entry, *depth)?,
        Command::Capabilities {
            command: Capabilities::List,
        } => query.capabilities(&cli.snapshot, repository)?,
        Command::Capabilities {
            command: Capabilities::Nominate { entry, title },
        } => query.capability_nominate(&cli.snapshot, repository, entry, title)?,
        Command::Capabilities {
            command: Capabilities::Reject { entry },
        } => query.capability_reject(&cli.snapshot, repository, entry)?,
        Command::Capabilities {
            command: Capabilities::Show { entry, depth },
        } => query.capability_show(&cli.snapshot, repository, entry, *depth)?,
        Command::Capabilities {
            command:
                Capabilities::StaleCheck {
                    repository: repo_name,
                    from,
                    to,
                },
        } => query.capability_stale_check(repo_name, from, to)?,
        Command::Docs {
            command: Docs::Render { out, capabilities },
        } => {
            if *capabilities {
                let f = query.docs_render(&cli.snapshot, repository, out)?;
                let c = query.docs_render_capabilities(&cli.snapshot, repository, out)?;
                json!({ "fiches": f, "capabilities": c })
            } else {
                query.docs_render(&cli.snapshot, repository, out)?
            }
        }
        Command::Behaviors {
            command: Behaviors::Reconstruct { entry },
        } => {
            query.reconstruct_behaviors(&cli.snapshot, repository, entry, cli.limit, cli.offset)?
        }
        Command::Rules {
            command: Rules::List { with_evidence },
        } => query.list_rules(
            &cli.snapshot,
            repository,
            *with_evidence,
            cli.limit,
            cli.offset,
        )?,
        Command::Rules {
            command:
                Rules::StaleCheck {
                    repository: repo_name,
                    from,
                    to,
                },
        } => query.rules_stale_check(repo_name, from, to)?,
    };
    Ok((value, status))
}

fn report_error(error: &Error, format: Format) -> i32 {
    eprintln!("{error}");
    if format == Format::Json {
        println!(
            "{}",
            json!({ "error": {
                "code": error.code, "message": error.message, "candidates": error.candidates
            }})
        );
    }
    error.exit_code()
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    // Parse failures must obey the same JSON envelope as execution failures.
    let json_requested = args.iter().enumerate().any(|(index, arg)| {
        arg == "--format=json"
            || (arg == "--format" && args.get(index + 1).is_some_and(|a| a == "json"))
    });
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                print!("{error}");
                return;
            }
            std::process::exit(report_error(
                &Error::new("E_USAGE", error.to_string()),
                if json_requested {
                    Format::Json
                } else {
                    Format::Text
                },
            ));
        }
    };
    let status = match execute(&cli) {
        Ok((value, status)) => {
            if cli.format == Format::Json {
                println!("{value}");
            } else if cli.format == Format::Yaml {
                let yaml = serde_yaml::to_string(&value).expect("YAML value");
                print!("{yaml}");
            } else if let Some(message) = value.get("message").and_then(Value::as_str) {
                println!("{message}");
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&value).expect("JSON value")
                );
            }
            status
        }
        Err(mut error) => {
            if matches!(
                &cli.command,
                Command::Ingest {
                    command: Ingest::Go(AdapterIngest { redact: true, .. })
                        | Ingest::Rust(AdapterIngest { redact: true, .. })
                        | Ingest::Java(AdapterIngest { redact: true, .. })
                }
            ) {
                error.message = "operation failed (details redacted)".into();
                error.candidates.clear();
            }
            report_error(&error, cli.format)
        }
    };
    std::process::exit(status);
}
