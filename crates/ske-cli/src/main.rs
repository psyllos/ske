use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use ske_common::{Error, Result};
use ske_query::Query;
use std::path::PathBuf;
mod consent;

#[derive(Clone, Copy, ValueEnum, PartialEq)]
enum Format {
    Text,
    Json,
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
}

#[derive(Subcommand)]
enum Ingest {
    Go {
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        profile: Option<PathBuf>,
        #[arg(long, default_value = "target/ske-adapter-go")]
        adapter: PathBuf,
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
    },
    Cir {
        #[arg(long, required = true)]
        input: Vec<PathBuf>,
        #[arg(long)]
        source_root: Option<PathBuf>,
        #[arg(long)]
        require_complete: bool,
    },
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
            command:
                Ingest::Go {
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
                },
        } => {
            let default_profile = repo.join("build-profile.json");
            let selected_profile = profile
                .as_ref()
                .or_else(|| default_profile.exists().then_some(&default_profile));
            let input = selected_profile
                .map(|path| ske_query::profile::read_profile(path))
                .transpose()?;
            let profile = ske_query::profile::normalize(input.as_deref())?;
            let limits = ske_query::RunLimits {
                max_message_bytes: *max_message_bytes,
                max_messages: *max_messages,
                timeout_ms: *timeout_ms,
                max_memory_mb: *max_memory_mb,
            };
            limits.validate()?;
            consent::ensure(path, repo, adapter, *yes_run_adapter)?;
            let value = query.ingest_go(&ske_query::GoOptions {
                repo,
                adapter,
                profile_json: &profile,
                limits: &limits,
                repository,
                redact: *redact,
            })?;
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
                    command: Ingest::Go { redact: true, .. }
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
