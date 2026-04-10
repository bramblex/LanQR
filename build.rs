#[cfg(windows)]
fn main() {
    if let Err(error) = build_windows_icon() {
        panic!("failed to build windows icon resources: {error}");
    }
}

#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
fn build_windows_icon() -> Result<(), Box<dyn std::error::Error>> {
    use ico::{IconDir, IconDirEntry, IconImage, ResourceType};
    use image::imageops::FilterType;
    use std::fs;
    use std::path::PathBuf;

    println!("cargo:rerun-if-changed=assets/icon.png");

    let icon_png_path = PathBuf::from("assets").join("icon.png");
    let base_image = image::open(&icon_png_path)?.into_rgba8();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let icon_ico_path = out_dir.join("lanqr.ico");
    let generated_icon_path = PathBuf::from("target").join("generated").join("LanQR.ico");

    let mut icon_dir = IconDir::new(ResourceType::Icon);
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let resized = image::imageops::resize(&base_image, size, size, FilterType::Lanczos3);
        let icon_image = IconImage::from_rgba_data(size, size, resized.into_raw());
        icon_dir.add_entry(IconDirEntry::encode(&icon_image)?);
    }

    let mut icon_file = fs::File::create(&icon_ico_path)?;
    icon_dir.write(&mut icon_file)?;
    fs::create_dir_all(
        generated_icon_path
            .parent()
            .ok_or("generated icon path has no parent")?,
    )?;
    fs::copy(&icon_ico_path, &generated_icon_path)?;

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_ico_path.to_string_lossy().as_ref());
    resource.compile()?;

    Ok(())
}
