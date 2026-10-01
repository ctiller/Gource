//! The `gource-serve` command line.

use crate::http::{ServeOptions, router};
use crate::store::{History, load_history};
use gource_vcs::VcsOptions;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;

pub const USAGE: &str =
    "usage: gource-serve [--bind ADDR] [--port N] [--web-root DIR] [--log-format FORMAT] PATH";

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub bind: String,
    pub port: u16,
    pub web_root: Option<PathBuf>,
    pub log_format: String,
    /// Log file or repository directory.
    pub path: String,
}

/// Parse the arguments (without the program name). `Err` holds the message
/// to print, including for `--help`.
pub fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        bind: "127.0.0.1".into(),
        port: 8080,
        web_root: None,
        log_format: String::new(),
        path: String::new(),
    };
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--bind" => args.bind = value()?,
            "--port" => args.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--web-root" => args.web_root = Some(value()?.into()),
            "--log-format" => args.log_format = value()?,
            "-h" | "--help" => return Err(USAGE.into()),
            _ if a.starts_with('-') => return Err(format!("unknown option {a}\n{USAGE}")),
            _ if args.path.is_empty() => args.path = a,
            _ => return Err(format!("unexpected argument {a}\n{USAGE}")),
        }
    }
    if args.path.is_empty() {
        return Err(USAGE.into());
    }
    Ok(args)
}

/// Load the history and serve it until `shutdown` completes. `listening`
/// is called with the bound address.
pub async fn run(
    args: Args,
    listening: impl FnOnce(SocketAddr),
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), String> {
    let options = VcsOptions {
        log_format: args.log_format,
        ..Default::default()
    };
    let commits = load_history(&args.path, &options)?;
    log::info!("{} commits from {}", commits.len(), args.path);
    let app = router(
        History::new(commits, false, options.hasher),
        &ServeOptions {
            web_root: args.web_root,
        },
    );
    let addr = format!("{}:{}", args.bind, args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
    listening(listener.local_addr().map_err(|e| e.to_string())?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn parse(a: &[&str]) -> Result<Args, String> {
        parse_args(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_options() {
        let a = parse(&[
            "--port",
            "9000",
            "--bind",
            "0.0.0.0",
            "--web-root",
            "w",
            "--log-format",
            "git",
            "repo",
        ])
        .unwrap();
        assert_eq!(
            a,
            Args {
                bind: "0.0.0.0".into(),
                port: 9000,
                web_root: Some("w".into()),
                log_format: "git".into(),
                path: "repo".into(),
            }
        );
        assert_eq!(parse(&["p"]).unwrap().port, 8080);
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse(&[]).is_err());
        assert!(parse(&["--port"]).unwrap_err().contains("needs a value"));
        assert!(parse(&["--port", "x", "p"]).is_err());
        assert!(
            parse(&["--nope", "p"])
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(parse(&["a", "b"]).unwrap_err().contains("unexpected"));
        assert_eq!(parse(&["--help"]).unwrap_err(), USAGE);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn serves_a_log_over_tcp() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("x.log");
        std::fs::write(&log, "1|alice|A|/a.rs\n").unwrap();
        let mut args = parse(&["--port", "0", log.to_str().unwrap()]).unwrap();
        let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(run(
            args.clone(),
            move |a| addr_tx.send(a).unwrap(),
            async move {
                stop_rx.await.ok();
            },
        ));
        let addr = addr_rx.await.unwrap();
        let body = tokio::task::spawn_blocking(move || {
            let mut s = std::net::TcpStream::connect(addr).unwrap();
            s.write_all(b"GET /info HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        })
        .await
        .unwrap();
        assert!(body.starts_with("HTTP/1.1 200"), "{body}");
        assert!(body.contains("\"commits\":1"), "{body}");
        stop_tx.send(()).unwrap();
        assert_eq!(server.await.unwrap(), Ok(()));

        // Errors: unreadable log, unusable address.
        args.path = "/nonexistent/x.log".into();
        assert!(run(args.clone(), |_| {}, async {}).await.is_err());
        args.path = log.to_str().unwrap().into();
        args.bind = "256.0.0.1".into();
        let err = run(args, |_| {}, async {}).await.unwrap_err();
        assert!(err.contains("cannot listen"), "{err}");
    }
}
