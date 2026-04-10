use std::path::Path;
use std::process::Command;

use tracing::info;

use crate::errors::{LanQrError, Result};

const FILE_KEY: &str = r"HKCU\Software\Classes\*\shell\LanQR";
const DIRECTORY_KEY: &str = r"HKCU\Software\Classes\Directory\shell\LanQR";
const MENU_TEXT: &str = "生成局域网二维码";

pub fn install(exe_path: &Path) -> Result<()> {
    let exe = exe_path.to_string_lossy();
    let command_value = format!("\"{exe}\" \"%1\"");
    let icon_value = icon_value_for_exe(exe_path);

    install_single(FILE_KEY, &command_value, &icon_value)?;
    install_single(DIRECTORY_KEY, &command_value, &icon_value)?;

    info!(exe_path = %exe, "installed context menu");
    Ok(())
}

fn icon_value_for_exe(exe_path: &Path) -> String {
    let sidecar_icon = exe_path.with_extension("ico");
    format!("\"{}\"", sidecar_icon.to_string_lossy())
}

pub fn uninstall() -> Result<()> {
    delete_key_if_exists(FILE_KEY)?;
    delete_key_if_exists(DIRECTORY_KEY)?;
    info!("uninstalled context menu");
    Ok(())
}

fn install_single(base_key: &str, command_value: &str, icon_value: &str) -> Result<()> {
    run_reg(&[
        "add",
        base_key,
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        MENU_TEXT,
        "/f",
    ])
    .map_err(|error| LanQrError::ContextMenuInstallFailed(error.to_string()))?;

    run_reg(&[
        "add",
        base_key,
        "/v",
        "Icon",
        "/t",
        "REG_SZ",
        "/d",
        icon_value,
        "/f",
    ])
    .map_err(|error| LanQrError::ContextMenuInstallFailed(error.to_string()))?;

    run_reg(&[
        "add",
        &format!(r"{base_key}\command"),
        "/ve",
        "/t",
        "REG_SZ",
        "/d",
        command_value,
        "/f",
    ])
    .map_err(|error| LanQrError::ContextMenuInstallFailed(error.to_string()))?;

    Ok(())
}

fn delete_key_if_exists(key: &str) -> Result<()> {
    if !key_exists(key)? {
        return Ok(());
    }

    run_reg(&["delete", key, "/f"])
        .map_err(|error| LanQrError::ContextMenuUninstallFailed(error.to_string()))?;
    Ok(())
}

fn key_exists(key: &str) -> Result<bool> {
    let output = Command::new("reg").args(["query", key]).output()?;
    Ok(output.status.success())
}

fn run_reg(args: &[&str]) -> Result<()> {
    let output = Command::new("reg").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = if !stderr.is_empty() { stderr } else { stdout };
    Err(LanQrError::Message(message))
}
