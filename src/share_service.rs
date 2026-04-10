use std::fmt::Write as _;
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use axum::extract::{OriginalUri, Path as AxumPath, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use rand::distributions::Alphanumeric;
use rand::Rng;
use tokio::runtime::Builder as RuntimeBuilder;
use tokio::sync::oneshot;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use tracing::{info, warn};

use crate::errors::{LanQrError, Result};
use crate::models::{ProcessState, ShareSession, ShareTarget};
use crate::network;

const ROUTE_PREFIX: &str = "/send/";

pub struct ShareService {
    server_thread: Option<JoinHandle<()>>,
    shutdown_tx: Option<oneshot::Sender<()>>,
    current_session: Option<ShareSession>,
}

impl ShareService {
    pub fn new() -> Self {
        Self {
            server_thread: None,
            shutdown_tx: None,
            current_session: None,
        }
    }

    pub fn start(&mut self, target: &ShareTarget, ip: Ipv4Addr) -> Result<ShareSession> {
        self.stop()?;

        let listener = network::bind_available_listener(ip)?;
        listener
            .set_nonblocking(true)
            .map_err(|error| LanQrError::ShareServiceStartFailed(error.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|error| LanQrError::ShareServiceStartFailed(error.to_string()))?
            .port();
        let route = random_route(10);
        let base_path = format!("{ROUTE_PREFIX}{route}");
        let final_url = build_share_url(ip, port, &route, target.is_dir);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let router = build_router(target.clone(), base_path.clone());

        info!(
            target = %target.original_path.display(),
            bind_ip = %ip,
            port,
            route = route.as_str(),
            url = final_url.as_str(),
            "starting built-in share service"
        );

        let server_thread = thread::spawn(move || {
            let runtime = match RuntimeBuilder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    warn!(error = %error, "failed to build tokio runtime for share service");
                    return;
                }
            };

            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        warn!(error = %error, "failed to adopt tcp listener for share service");
                        return;
                    }
                };

                let server = axum::serve(listener, router.into_make_service())
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    });

                if let Err(error) = server.await {
                    warn!(error = %error, "share service stopped with server error");
                }
            });
        });

        let session = ShareSession {
            ip,
            port,
            route,
            url: final_url,
        };

        self.server_thread = Some(server_thread);
        self.shutdown_tx = Some(shutdown_tx);
        self.current_session = Some(session.clone());

        Ok(session)
    }

    pub fn restart(&mut self, target: &ShareTarget, ip: Ipv4Addr) -> Result<ShareSession> {
        self.start(target, ip)
    }

    pub fn poll_status(&mut self) -> ProcessState {
        let Some(server_thread) = self.server_thread.as_ref() else {
            return ProcessState::NotStarted;
        };

        if !server_thread.is_finished() {
            return ProcessState::Running;
        }

        if let Some(server_thread) = self.server_thread.take() {
            if let Err(error) = server_thread.join() {
                warn!(error = ?error, "share service thread panicked");
            }
        }

        self.shutdown_tx = None;
        self.current_session = None;
        ProcessState::Exited(None)
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());

            if let Some(session) = self.current_session.as_ref() {
                let _ = TcpStream::connect(SocketAddrV4::new(session.ip, session.port));
            }
        }

        if let Some(server_thread) = self.server_thread.take() {
            if let Err(error) = server_thread.join() {
                warn!(error = ?error, "share service thread panicked during stop");
            }
        }

        self.current_session = None;
        Ok(())
    }
}

impl Drop for ShareService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn build_router(target: ShareTarget, base_path: String) -> Router {
    if target.is_dir {
        build_directory_router(target.original_path, base_path)
    } else {
        build_file_router(target.original_path, base_path)
    }
}

fn build_file_router(file_path: PathBuf, base_path: String) -> Router {
    let trailing_path = format!("{base_path}/");
    let redirect_path = base_path.clone();
    let state = Arc::new(FileShareState { path: file_path });

    Router::new()
        .route(&base_path, get(file_handler))
        .route(
            &trailing_path,
            get(move || {
                let redirect_path = redirect_path.clone();
                async move { Redirect::permanent(&redirect_path) }
            }),
        )
        .with_state(state)
}

fn build_directory_router(root: PathBuf, base_path: String) -> Router {
    let state = Arc::new(DirectoryShareState { root });
    let slash_path = format!("{base_path}/");
    let wildcard_path = format!("{base_path}/*tail");
    let redirect_target = slash_path.clone();

    Router::new()
        .route(
            &base_path,
            get(move || {
                let redirect_target = redirect_target.clone();
                async move { Redirect::permanent(&redirect_target) }
            }),
        )
        .route(&slash_path, get(directory_root_handler))
        .route(&wildcard_path, get(directory_tail_handler))
        .with_state(state)
}

async fn directory_root_handler(
    State(state): State<Arc<DirectoryShareState>>,
    OriginalUri(uri): OriginalUri,
    request: Request,
) -> Response {
    serve_directory_entry(state, PathBuf::new(), uri.path().to_string(), request).await
}

async fn file_handler(
    State(state): State<Arc<FileShareState>>,
    request: Request,
) -> Response {
    serve_file_response(state.path.clone(), request).await
}

async fn directory_tail_handler(
    State(state): State<Arc<DirectoryShareState>>,
    AxumPath(tail): AxumPath<String>,
    OriginalUri(uri): OriginalUri,
    request: Request,
) -> Response {
    let relative = match normalize_relative_path(&tail) {
        Some(relative) => relative,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    serve_directory_entry(state, relative, uri.path().to_string(), request).await
}

async fn serve_directory_entry(
    state: Arc<DirectoryShareState>,
    relative: PathBuf,
    request_path: String,
    request: Request,
) -> Response {
    let filesystem_path = state.root.join(&relative);
    let metadata = match fs::metadata(&filesystem_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(error) => {
            warn!(path = %filesystem_path.display(), error = %error, "failed to read shared path metadata");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if metadata.is_dir() {
        if !request_path.ends_with('/') {
            return Redirect::permanent(&format!("{request_path}/")).into_response();
        }

        match build_directory_listing_html(&filesystem_path, &relative) {
            Ok(html) => Html(html).into_response(),
            Err(error) => {
                warn!(path = %filesystem_path.display(), error = %error, "failed to build directory listing");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    } else {
        serve_file_response(filesystem_path, request).await
    }
}

async fn serve_file_response(path: PathBuf, request: Request) -> Response {
    let disposition = build_content_disposition_header(&path);
    match ServeFile::new(path).oneshot(request).await {
        Ok(mut response) => {
            if let Some(disposition) = disposition {
                response.headers_mut().insert(header::CONTENT_DISPOSITION, disposition);
            }
            response.into_response()
        }
        Err(error) => match error {},
    }
}

fn build_directory_listing_html(path: &Path, relative: &Path) -> std::io::Result<String> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        entries.push(DirectoryEntry {
            name,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let title = if relative.components().next().is_none() {
        "/".to_string()
    } else {
        format!("/{}", relative.to_string_lossy().replace('\\', "/"))
    };

    let mut html = String::new();
    let _ = write!(
        html,
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{}</title><style>\
body{{font-family:Segoe UI,system-ui,sans-serif;margin:0;background:#f5f7fb;color:#1f2937}}\
main{{max-width:880px;margin:0 auto;padding:24px}}\
h1{{font-size:22px;margin:0 0 12px}}\
ul{{list-style:none;padding:0;margin:16px 0 0}}\
li{{background:#fff;border:1px solid #dbe3f0;border-radius:10px;margin:8px 0}}\
a{{display:block;padding:14px 16px;color:#0f172a;text-decoration:none;word-break:break-all}}\
a:hover{{background:#eef4ff}}\
.meta{{color:#64748b;font-size:13px}}\
</style></head><body><main><h1>目录 {}</h1><div class=\"meta\">LanQR 静态分享</div><ul>",
        html_escape(&title),
        html_escape(&title),
    );

    if relative.components().next().is_some() {
        html.push_str("<li><a href=\"../\">..<div class=\"meta\">返回上级目录</div></a></li>");
    }

    for entry in entries {
        let encoded = percent_encode_path_segment(&entry.name);
        let suffix = if entry.is_dir { "/" } else { "" };
        let label = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        let meta = if entry.is_dir {
            "目录".to_string()
        } else {
            format!("文件 · {} 字节", entry.size)
        };

        let _ = write!(
            html,
            "<li><a href=\"{}{}\">{}<div class=\"meta\">{}</div></a></li>",
            encoded,
            suffix,
            html_escape(&label),
            html_escape(&meta),
        );
    }

    html.push_str("</ul></main></body></html>");
    Ok(html)
}

fn normalize_relative_path(input: &str) -> Option<PathBuf> {
    if input.is_empty() {
        return Some(PathBuf::new());
    }

    let mut result = PathBuf::new();
    for segment in input.split('/') {
        if segment.is_empty() {
            continue;
        }

        if segment.contains('\\') || segment.contains('\0') {
            return None;
        }

        let component_path = Path::new(segment);
        if component_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }

        result.push(segment);
    }

    Some(result)
}

fn percent_encode_path_segment(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(*byte as char);
        } else {
            let _ = write!(encoded, "%{:02X}", byte);
        }
    }
    encoded
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn random_route(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(length)
        .collect()
}

fn build_content_disposition_header(path: &Path) -> Option<HeaderValue> {
    let filename = path.file_name()?.to_string_lossy();
    let fallback = filename
        .chars()
        .map(|ch| match ch {
            '"' | '\\' => '_',
            ch if ch.is_ascii() && !ch.is_ascii_control() => ch,
            _ => '_',
        })
        .collect::<String>();
    let encoded = percent_encode_path_segment(&filename);
    let value = format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}");
    HeaderValue::from_str(&value).ok()
}

fn build_share_url(ip: Ipv4Addr, port: u16, route: &str, is_dir: bool) -> String {
    if is_dir {
        format!("http://{ip}:{port}{ROUTE_PREFIX}{route}/")
    } else {
        format!("http://{ip}:{port}{ROUTE_PREFIX}{route}")
    }
}

struct DirectoryShareState {
    root: PathBuf,
}

struct FileShareState {
    path: PathBuf,
}

struct DirectoryEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{header, Request};

    #[test]
    fn file_route_supports_partial_content() {
        let temp_dir = std::env::temp_dir().join(format!("lanqr-test-{}", random_route(8)));
        fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("large.bin");
        let expected = vec![0x5a; 256 * 1024];
        fs::write(&file_path, &expected).unwrap();

        let runtime = RuntimeBuilder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let app = build_file_router(file_path.clone(), "/send/test".to_string());
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/send/test")
                        .header(header::RANGE, "bytes=0-65535")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                response.headers().get(header::CONTENT_RANGE).unwrap(),
                "bytes 0-65535/262144"
            );
            assert_eq!(
                response.headers().get(header::CONTENT_DISPOSITION).unwrap(),
                "attachment; filename=\"large.bin\"; filename*=UTF-8''large.bin"
            );
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert_eq!(body.len(), 65536);
            assert_eq!(body.as_ref(), &expected[..65536]);
        });

        let _ = fs::remove_file(&file_path);
        let _ = fs::remove_dir(&temp_dir);
    }

    #[test]
    fn directory_listing_keeps_navigation() {
        let temp_dir = std::env::temp_dir().join(format!("lanqr-test-{}", random_route(8)));
        let nested = temp_dir.join("sub");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("hello.txt"), b"hello").unwrap();

        let html = build_directory_listing_html(&nested, Path::new("sub")).unwrap();
        assert!(html.contains("../"));
        assert!(html.contains("hello.txt"));

        let _ = fs::remove_file(nested.join("hello.txt"));
        let _ = fs::remove_dir(&nested);
        let _ = fs::remove_dir(&temp_dir);
    }
}
