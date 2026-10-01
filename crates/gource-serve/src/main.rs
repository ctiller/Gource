//! `gource-serve [--bind ADDR] [--port N] [--web-root DIR] [--log-format F] PATH`

use gource_serve::cli;

#[tokio::main]
async fn main() {
    let args = cli::parse_args(std::env::args().skip(1)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let listening = |addr| eprintln!("gource-serve: listening on http://{addr}");
    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
    };
    if let Err(e) = cli::run(args, listening, shutdown).await {
        eprintln!("gource-serve: {e}");
        std::process::exit(1);
    }
}
