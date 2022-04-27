#![forbid(unsafe_code)]

use clap::Parser;

#[derive(Parser)]
#[clap(about = "Just a easy sever.")]
struct Args {
    /// Set the working root directory
    #[clap(short = 'd', long = "directory", default_value = ".")]
    dir: String,

    /// Set the listening IP address
    #[clap(short = 'a', long = "address", default_value = "0.0.0.0")]
    ip: std::net::IpAddr,

    /// Set the listening port
    #[clap(short = 'p', long = "port", default_value = "9999")]
    port: u16,
}

struct ServerConfig {
    work_path: PathBuf,
}

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    response::{Html, IntoResponse},
    routing::get_service,
    Router,
};
use std::{path::PathBuf, sync::Arc};
use tokio::fs;
use tower_http::{add_extension::AddExtensionLayer, services::ServeDir};

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let path = PathBuf::from(args.dir);
    let root_path = fs::canonicalize(&path).await.expect("Path Error");
    println!("Working on {:?}", root_path);

    let addr = std::net::SocketAddr::from((args.ip, args.port));
    println!("Please visit http://{}/", addr);

    let config = ServerConfig {
        work_path: root_path.clone(),
    };

    let app = Router::new()
        .nest(
            "",
            get_service(ServeDir::new(root_path)).handle_error(
                |error: std::io::Error| async move {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Service Error: {}", error),
                    )
                },
            ),
        )
        .layer(middleware::from_fn(dir_handler))
        .layer(AddExtensionLayer::new(Arc::new(config)));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("Server Error");
}

struct EntryInfo {
    name: String,
    is_dir: bool,
}

use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    entry_list: Vec<EntryInfo>,
}

async fn dir_handler(
    req: Request<Body>,
    next: middleware::Next<Body>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let mut work_path = match req.extensions().get::<Arc<ServerConfig>>() {
        Some(config) => config.work_path.to_owned(),
        None => return Err((StatusCode::INTERNAL_SERVER_ERROR, "Path Error".to_string())),
    };
    let uri = req.uri().clone();
    println!("{:?}", uri);

    let res = next.run(req).await;
    if res.status() != StatusCode::NOT_FOUND {
        return Ok(res);
    };

    let uri = uri.to_string().trim_start_matches('/').to_owned();
    let decoded_uri = percent_encoding::percent_decode_str(&uri).decode_utf8_lossy();
    work_path.push(&*decoded_uri);
    println!("{:?}", work_path);
    if work_path.is_dir() {
        if let Ok(entry_list) = list_entry(&work_path).await {
            let entry_list = IndexTemplate { entry_list };
            if let Ok(body_string) = entry_list.render() {
                return Ok(Html(body_string).into_response());
            } else {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Render Error".to_string(),
                ));
            };
        };
    };

    Ok(res)
}

async fn list_entry(path: &PathBuf) -> std::io::Result<Vec<EntryInfo>> {
    let mut entry_list = Vec::new();
    let mut dir = fs::read_dir(&path).await?;
    while let Some(entry) = dir.next_entry().await? {
        let file_name = entry.file_name();
        let metadata = entry.metadata().await?;

        entry_list.push(EntryInfo {
            name: file_name.to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
        })
    }
    Ok(entry_list)
}
