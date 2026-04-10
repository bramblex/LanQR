use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::errors::{LanQrError, Result};

pub fn init_logging() -> Result<(PathBuf, WorkerGuard)> {
    let project_dirs = ProjectDirs::from("cn", "LanQR", "LanQR")
        .ok_or_else(|| LanQrError::Message("无法确定本地应用数据目录".to_string()))?;

    let log_dir = project_dirs.data_local_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    let file_name = format!("lanqr-{}.log", unix_timestamp());
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join(file_name))?;

    let (writer, guard) = tracing_appender::non_blocking(log_file);

    let subscriber = tracing_subscriber::fmt()
        .with_writer(BoxMakeWriter::new(writer))
        .with_ansi(false)
        .with_target(true)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);

    Ok((log_dir, guard))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
