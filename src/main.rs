#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod context_menu;
mod errors;
mod logging;
mod models;
mod network;
mod qr;
mod share_service;

use std::env;
use std::fs;
use std::path::PathBuf;

use eframe::egui;
use tracing::{error, info};

use crate::app::LanQrApp;
use crate::context_menu::{install, uninstall};
use crate::errors::{LanQrError, Result};
use crate::models::{LaunchMode, ShareTarget};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        error!(error = %error, "fatal startup error");
    }
}

fn run() -> Result<()> {
    let (_log_dir, _guard) = logging::init_logging()?;
    let exe_path = env::current_exe()?;
    let args: Vec<_> = env::args_os().skip(1).collect();

    info!(args = ?args, exe = %exe_path.display(), "LanQR starting");

    match parse_launch_mode(&args)? {
        ParsedLaunch::Gui(launch_mode) => launch_gui(launch_mode, exe_path),
        ParsedLaunch::InstallContextMenu => {
            install(&exe_path)?;
            info!("context menu installed from cli");
            Ok(())
        }
        ParsedLaunch::UninstallContextMenu => {
            uninstall()?;
            info!("context menu uninstalled from cli");
            Ok(())
        }
    }
}

fn launch_gui(launch_mode: LaunchMode, exe_path: PathBuf) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("邻享码")
            .with_inner_size([520.0, 720.0])
            .with_min_inner_size([480.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        "邻享码",
        options,
        Box::new(move |cc| Ok(Box::new(LanQrApp::new(cc, launch_mode.clone(), exe_path.clone())))),
    )
    .map_err(|error| LanQrError::Message(format!("启动 GUI 失败：{error}")))?;

    Ok(())
}

enum ParsedLaunch {
    Gui(LaunchMode),
    InstallContextMenu,
    UninstallContextMenu,
}

fn parse_launch_mode(args: &[std::ffi::OsString]) -> Result<ParsedLaunch> {
    if args.is_empty() {
        return Ok(ParsedLaunch::Gui(LaunchMode::Idle));
    }

    if args.len() == 1 {
        match args[0].to_string_lossy().as_ref() {
            "--install-context-menu" => return Ok(ParsedLaunch::InstallContextMenu),
            "--uninstall-context-menu" => return Ok(ParsedLaunch::UninstallContextMenu),
            _ => {}
        }
    }

    let target_path = PathBuf::from(&args[0]);
    let target = validate_target(target_path)?;
    Ok(ParsedLaunch::Gui(LaunchMode::Share(target)))
}

fn validate_target(path: PathBuf) -> Result<ShareTarget> {
    if !path.exists() {
        return Err(LanQrError::TargetNotFound(path));
    }

    let metadata = fs::metadata(&path).map_err(|error| {
        if matches!(error.kind(), std::io::ErrorKind::PermissionDenied) {
            LanQrError::TargetAccessDenied(path.clone())
        } else {
            LanQrError::Message(format!("读取目标路径失败：{error}"))
        }
    })?;

    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());

    Ok(ShareTarget {
        original_path: path.clone(),
        display_name,
        is_dir: metadata.is_dir(),
    })
}
