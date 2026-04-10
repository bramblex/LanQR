use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rand::distributions::Alphanumeric;
use rand::Rng;
use tracing::{info, warn};

use crate::errors::{LanQrError, Result};
use crate::models::{ProcessState, ShareSession, ShareTarget};
use crate::network;

const ROUTE_PREFIX: &str = "/send/";
const POLL_INTERVAL: Duration = Duration::from_millis(100);
const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(3);

pub struct ShareService {
    server_thread: Option<JoinHandle<()>>,
    stop_flag: Option<Arc<AtomicBool>>,
    current_session: Option<ShareSession>,
}

impl ShareService {
    pub fn new() -> Self {
        Self {
            server_thread: None,
            stop_flag: None,
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
        let final_url = build_share_url(ip, port, &route, target.is_dir);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let thread_target = Arc::new(target.clone());
        let thread_route = route.clone();
        let thread_stop = Arc::clone(&stop_flag);

        info!(
            target = %target.original_path.display(),
            bind_ip = %ip,
            port,
            route = route.as_str(),
            url = final_url.as_str(),
            "starting built-in share service"
        );

        let server_thread = thread::spawn(move || {
            run_server(listener, thread_stop, thread_target, thread_route);
        });

        let session = ShareSession {
            ip,
            port,
            route,
            url: final_url,
        };

        self.server_thread = Some(server_thread);
        self.stop_flag = Some(stop_flag);
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

        self.stop_flag = None;
        self.current_session = None;
        ProcessState::Exited(None)
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(stop_flag) = self.stop_flag.take() {
            stop_flag.store(true, Ordering::Relaxed);

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

fn run_server(
    listener: TcpListener,
    stop_flag: Arc<AtomicBool>,
    target: Arc<ShareTarget>,
    route: String,
) {
    let base_path = format!("{ROUTE_PREFIX}{route}");
    info!(base_path = base_path.as_str(), "share service listening");

    while !stop_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, peer)) => {
                let request_target = Arc::clone(&target);
                let request_base = base_path.clone();
                thread::spawn(move || {
                    if let Err(error) = handle_client(stream, &request_target, &request_base) {
                        warn!(peer = %peer, error = %error, "request handling failed");
                    }
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => {
                warn!(error = %error, "share service listener stopped unexpectedly");
                break;
            }
        }
    }

    info!("share service stopped");
}

fn handle_client(mut stream: TcpStream, target: &ShareTarget, base_path: &str) -> io::Result<()> {
    let request = read_request(&stream)?;

    if request.method != "GET" && request.method != "HEAD" {
        return write_plain_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Method Not Allowed",
            request.method == "HEAD",
        );
    }

    let request_path = strip_query_and_fragment(&request.path);

    if target.is_dir {
        handle_directory_request(&mut stream, target, base_path, request_path, request.method == "HEAD")
    } else {
        handle_file_request(&mut stream, target, base_path, request_path, request.method == "HEAD")
    }
}

fn handle_file_request(
    stream: &mut TcpStream,
    target: &ShareTarget,
    base_path: &str,
    request_path: &str,
    head_only: bool,
) -> io::Result<()> {
    if request_path != base_path && request_path != format!("{base_path}/") {
        return write_plain_response(stream, "404 Not Found", "text/plain; charset=utf-8", b"Not Found", head_only);
    }

    serve_file(stream, &target.original_path, head_only)
}

fn handle_directory_request(
    stream: &mut TcpStream,
    target: &ShareTarget,
    base_path: &str,
    request_path: &str,
    head_only: bool,
) -> io::Result<()> {
    if request_path == base_path {
        return write_redirect(stream, &format!("{base_path}/"));
    }

    let Some(mut relative) = request_path.strip_prefix(base_path) else {
        return write_plain_response(stream, "404 Not Found", "text/plain; charset=utf-8", b"Not Found", head_only);
    };

    if !relative.starts_with('/') {
        return write_plain_response(stream, "404 Not Found", "text/plain; charset=utf-8", b"Not Found", head_only);
    }

    relative = &relative[1..];
    let normalized = match normalize_relative_path(relative) {
        Some(path) => path,
        None => {
            return write_plain_response(
                stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                b"Invalid Path",
                head_only,
            );
        }
    };

    let filesystem_path = target.original_path.join(&normalized);
    let metadata = match fs::metadata(&filesystem_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return write_plain_response(stream, "404 Not Found", "text/plain; charset=utf-8", b"Not Found", head_only);
        }
        Err(error) => return Err(error),
    };

    if metadata.is_dir() {
        if !request_path.ends_with('/') {
            return write_redirect(stream, &format!("{request_path}/"));
        }

        return serve_directory_listing(stream, &filesystem_path, &normalized, head_only);
    }

    serve_file(stream, &filesystem_path, head_only)
}

fn serve_file(stream: &mut TcpStream, path: &Path, head_only: bool) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    let content_length = metadata.len();
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let content_type = detect_content_type(path);
    let disposition = build_content_disposition(&filename);

    write_headers(
        stream,
        "200 OK",
        &[
            ("Content-Type", content_type),
            ("Content-Length", content_length.to_string()),
            ("Content-Disposition", disposition),
            ("Cache-Control", "no-store".to_string()),
        ],
    )?;

    if head_only {
        return Ok(());
    }

    let mut file = File::open(path)?;
    io::copy(&mut file, stream)?;
    Ok(())
}

fn serve_directory_listing(
    stream: &mut TcpStream,
    path: &Path,
    relative: &Path,
    head_only: bool,
) -> io::Result<()> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        entries.push(DirectoryEntry {
            name,
            is_dir: metadata.is_dir(),
            path: entry_path,
        });
    }

    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });

    let body = build_directory_listing_html(relative, &entries);
    write_plain_response(
        stream,
        "200 OK",
        "text/html; charset=utf-8",
        body.as_bytes(),
        head_only,
    )
}

fn build_directory_listing_html(relative: &Path, entries: &[DirectoryEntry]) -> String {
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
</style></head><body><main><h1>目录 {}</h1><div class=\"meta\">LanQR 内置静态分享服务</div><ul>",
        html_escape(&title),
        html_escape(&title),
    );

    if relative.components().next().is_some() {
        html.push_str("<li><a href=\"../\">..</a></li>");
    }

    for entry in entries {
        let encoded = percent_encode_path_segment(&entry.name);
        let suffix = if entry.is_dir { "/" } else { "" };
        let label = if entry.is_dir {
            format!("{}/", entry.name)
        } else {
            entry.name.clone()
        };
        let size_meta = if entry.is_dir {
            "目录".to_string()
        } else {
            match entry.path.metadata() {
                Ok(metadata) => format!("文件 · {} 字节", metadata.len()),
                Err(_) => "文件".to_string(),
            }
        };
        let _ = write!(
            html,
            "<li><a href=\"{}{}\">{}<div class=\"meta\">{}</div></a></li>",
            encoded,
            suffix,
            html_escape(&label),
            html_escape(&size_meta),
        );
    }

    html.push_str("</ul></main></body></html>");
    html
}

fn read_request(stream: &TcpStream) -> io::Result<HttpRequest> {
    stream.set_read_timeout(Some(HEADER_READ_TIMEOUT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    let bytes = reader.read_line(&mut request_line)?;
    if bytes == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "empty request"));
    }

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 || line == "\r\n" {
            break;
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or("/").to_string();

    Ok(HttpRequest { method, path })
}

fn write_redirect(stream: &mut TcpStream, location: &str) -> io::Result<()> {
    write_headers(
        stream,
        "301 Moved Permanently",
        &[
            ("Location", location.to_string()),
            ("Content-Length", "0".to_string()),
            ("Cache-Control", "no-store".to_string()),
        ],
    )
}

fn write_plain_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    head_only: bool,
) -> io::Result<()> {
    write_headers(
        stream,
        status,
        &[
            ("Content-Type", content_type.to_string()),
            ("Content-Length", body.len().to_string()),
            ("Cache-Control", "no-store".to_string()),
        ],
    )?;

    if !head_only {
        stream.write_all(body)?;
    }

    Ok(())
}

fn write_headers(stream: &mut TcpStream, status: &str, headers: &[(&str, String)]) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\n")?;
    write!(stream, "Connection: close\r\n")?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")?;
    stream.flush()
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

        let decoded = percent_decode(segment)?;
        if decoded.contains('\\') || decoded.contains('\0') {
            return None;
        }

        let component_path = Path::new(&decoded);
        if component_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return None;
        }

        result.push(decoded);
    }

    Some(result)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let value = decode_hex_pair(bytes[index + 1], bytes[index + 2])?;
                decoded.push(value);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8(decoded).ok()
}

fn decode_hex_pair(left: u8, right: u8) -> Option<u8> {
    let left = (left as char).to_digit(16)?;
    let right = (right as char).to_digit(16)?;
    Some(((left << 4) | right) as u8)
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

fn build_content_disposition(filename: &str) -> String {
    let fallback = filename
        .chars()
        .map(|ch| match ch {
            '"' | '\\' => '_',
            ch if ch.is_ascii() && !ch.is_ascii_control() => ch,
            _ => '_',
        })
        .collect::<String>();

    format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        fallback,
        percent_encode_path_segment(filename)
    )
}

fn detect_content_type(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|item| item.to_str())
        .map(|item| item.to_ascii_lowercase());

    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn strip_query_and_fragment(path: &str) -> &str {
    path.split(['?', '#']).next().unwrap_or(path)
}

fn random_route(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .map(char::from)
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(length)
        .collect()
}

fn build_share_url(ip: Ipv4Addr, port: u16, route: &str, is_dir: bool) -> String {
    if is_dir {
        format!("http://{ip}:{port}{ROUTE_PREFIX}{route}/")
    } else {
        format!("http://{ip}:{port}{ROUTE_PREFIX}{route}")
    }
}

struct HttpRequest {
    method: String,
    path: String,
}

struct DirectoryEntry {
    name: String,
    is_dir: bool,
    path: PathBuf,
}
