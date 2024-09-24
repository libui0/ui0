use axum::http::header::CONTENT_TYPE;
use axum::http::Response;
use axum::{response::Html, routing::get, Router};
use clap::{Args, Parser};
use console::{Key::Char, Term};
use oxc_allocator::Allocator;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{future::pending, io::ErrorKind::AddrInUse};
use tokio::net::TcpListener;
use ui0::Bundle;

const HTML: &str = "<!doctype html>
<html>
<head>
<script type=\"module\" src=\"/index.js\"></script>
<title>UI0</title>
</head>
<body>
<strong>UI0</strong>
</body>
</html>";

async fn preview() {
    let components = ui0_components::load!();
    let allocator = Allocator::default();
    let mut bundle = Bundle::new(&allocator);
    for (_name, source) in components {
        bundle.add(source);
    }
    let js = bundle.js();
    let app = Router::new()
        .route(
            "/index.js",
            get(|| async {
                Response::builder()
                    .header(CONTENT_TYPE, "application/javascript")
                    .body(js)
                    .unwrap()
            }),
        )
        .fallback(get(|| async { Html(HTML) }));

    for port in 1703..=1998 {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let bind = TcpListener::bind(addr);
        match bind.await {
            Ok(listener) => {
                let task = axum::serve(listener, app.clone()).with_graceful_shutdown(async {
                    let term = Term::stdout();
                    if term.write_line("q to quit").is_ok() {
                        loop {
                            if let Ok(Char('q')) = term.read_key() {
                                break;
                            }
                        }
                    } else {
                        pending().await
                    }
                });
                let _ = webbrowser::open(format!("http://{}/", &addr).as_str());

                if let Err(err) = task.await {
                    eprintln!("Server error: {}", err);
                }

                return;
            }
            Err(err) => {
                if err.kind() != AddrInUse {
                    eprintln!("Server error: {}", err);
                    return;
                }
            }
        }
    }
}

#[derive(Parser)]
#[command(name = "ui0")]
#[command(bin_name = "ui0")]
enum Cli {
    Preview(Preview),
}

#[derive(Args)]
struct Preview {
    components: Vec<String>,
}

fn main() {
    match Cli::parse() {
        Cli::Preview(_preview_args) => {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async { preview().await });
        }
    }
}
