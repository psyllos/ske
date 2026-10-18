//! Explicit opt-in enrichment, separate from the deterministic `ske` CLI.
use ske_api::{llm::LlmClient, llm_consent::Consent, llm_http::HttpTransport};
use std::{path::PathBuf, sync::Arc};

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut db = None;
    let mut endpoint = None;
    let mut model = None;
    let mut snapshot = "latest".to_string();
    let mut repository = None;
    let mut consent = None;
    let mut source = Vec::new();
    let mut allow = false;
    let mut record = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next(),
            "--endpoint" => endpoint = args.next(),
            "--model" => model = args.next(),
            "--snapshot" => snapshot = args.next().ok_or("missing snapshot")?,
            "--repository" => repository = args.next(),
            "--consent" => consent = args.next().map(PathBuf::from),
            "--source" => source.push(std::fs::read_to_string(
                args.next().ok_or("missing source")?,
            )?),
            "--allow-source-upload" => allow = true,
            "--record-consent" => record = true,
            "--help" => {
                println!("ske-llm --db PATH --endpoint URL --model NAME [--snapshot ID] [--repository NAME] [--source FILE --allow-source-upload --consent PATH --record-consent]\n--record-consent explicitly approves these exact source bytes for this endpoint/repository/snapshot; credentials use SKE_LLM_API_KEY.");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let query = ske_query::Query::open(db.ok_or("--db required")?)?;
    let mut client = LlmClient::new()
        .enabled()
        .with_model(model.ok_or("--model required")?)
        .with_transport(Arc::new(HttpTransport::new(
            std::env::var("SKE_LLM_API_KEY").ok(),
        )));
    client.base_url = endpoint.ok_or("--endpoint required")?;
    client.allow_source_upload = allow;
    client.consent_path = consent;
    if record {
        if !allow || source.is_empty() {
            return Err("recording consent requires --allow-source-upload and --source".into());
        }
        let facts = query.list_rules(&snapshot, repository.as_deref(), false, 1, 0)?;
        Consent::scope(
            &client.base_url,
            facts["snapshot_id"].as_i64().ok_or("missing snapshot")?,
            repository.as_deref(),
            &source,
        )
        .record(client.consent_path.as_deref().ok_or("--consent required")?)?;
    }
    println!(
        "{}",
        client.enrich_rules(&query, &snapshot, repository.as_deref(), source)?
    );
    Ok(())
}
