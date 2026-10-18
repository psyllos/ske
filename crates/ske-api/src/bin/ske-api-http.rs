use std::net::{SocketAddr, TcpListener};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let db = args
        .next()
        .ok_or("usage: ske-api-http DATABASE [127.0.0.1:PORT]")?;
    let address: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:8765".into())
        .parse()?;
    if args.next().is_some() || !address.ip().is_loopback() {
        return Err("loopback address required".into());
    }
    let api = ske_api::ApiService::open(db)?;
    let listener = TcpListener::bind(address)?;
    eprintln!("SKE HTTP JSON listening on {}", listener.local_addr()?);
    loop {
        if let Err(error) = ske_api::http::serve_one(&listener, &api) {
            eprintln!("HTTP connection: {error}");
        }
    }
}
