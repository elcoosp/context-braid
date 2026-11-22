const APP_ICON_PNG: &[u8] = include_bytes!("../../../docs/icon-squircle.png");
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn default_install_bin() -> Result<PathBuf> {
    let app_support = dirs::data_local_dir()
        .context("Impossible de résoudre ~/Library/Application Support")?
        .join("ContextBraid/bin");
    fs::create_dir_all(&app_support)?;
    Ok(app_support.join("braid"))
}

fn collect_existing_origins(manifest_path: &Path, set: &mut BTreeSet<String>) {
    if let Ok(raw) = fs::read_to_string(manifest_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(arr) = val.get("allowed_origins").and_then(|v| v.as_array())
    {
        for item in arr {
            if let Some(s) = item.as_str()
                && s.starts_with("chrome-extension://")
            {
                set.insert(s.to_string());
            }
        }
    }
}

pub fn install_self(extra_ext_ids: &[String]) -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let target_bin = default_install_bin()?;

    if current_exe != target_bin {
        let _ = fs::remove_file(&target_bin);
        fs::copy(&current_exe, &target_bin)?;
        let mut perms = fs::metadata(&target_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_bin, perms)?;
    }
    // Re-signe en ad-hoc le Mach-O arm64 (obligatoire sur Apple Silicon après copie)
    let _ = Command::new("codesign")
        .args(["-s", "-", "--force", &target_bin.to_string_lossy()])
        .output();

    let host_link = target_bin
        .parent()
        .context("Parent bin dir missing")?
        .join("braid-host");
    let _ = fs::remove_file(&host_link);
    std::os::unix::fs::symlink(&target_bin, &host_link)?;

    // Build self-contained macOS .app bundle with native .icns so macOS notifications display the ContextBraid icon
    let app_support = dirs::data_local_dir().context("Application Support introuvable")?;
    let base_dir = app_support.join("ContextBraid");
    let app_dir = base_dir.join("ContextBraid.app");
    let app_contents = app_dir.join("Contents");
    let app_macos = app_contents.join("MacOS");
    let app_res = app_contents.join("Resources");
    fs::create_dir_all(&app_macos)?;
    fs::create_dir_all(&app_res)?;

    let app_exec = app_macos.join("braid");
    let _ = fs::remove_file(&app_exec);
    fs::copy(&target_bin, &app_exec)?;
    let mut bperms = fs::metadata(&app_exec)?.permissions();
    bperms.set_mode(0o755);
    fs::set_permissions(&app_exec, bperms)?;

    let icon_png = base_dir.join("icon.png");
    fs::write(&icon_png, APP_ICON_PNG)?;

    let iconset_dir = std::env::temp_dir().join("ContextBraid.iconset");
    let _ = fs::remove_dir_all(&iconset_dir);
    if fs::create_dir_all(&iconset_dir).is_ok() {
        for (size, name) in [
            (16, "icon_16x16.png"),
            (32, "icon_16x16@2x.png"),
            (32, "icon_32x32.png"),
            (64, "icon_32x32@2x.png"),
            (128, "icon_128x128.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
            (256, "icon_128x128@2x.png"),
            (256, "icon_256x256.png"),
            (512, "icon_256x256@2x.png"),
            (512, "icon_512x512.png"),
            (1024, "icon_512x512@2x.png"),
        ] {
            let _ = Command::new("sips")
                .args([
                    "-z",
                    &size.to_string(),
                    &size.to_string(),
                    &icon_png.to_string_lossy(),
                    "--out",
                    &iconset_dir.join(name).to_string_lossy(),
                ])
                .output();
        }
        let _ = Command::new("iconutil")
            .args([
                "-c",
                "icns",
                &iconset_dir.to_string_lossy(),
                "-o",
                &app_res.join("AppIcon.icns").to_string_lossy(),
            ])
            .output();
        let _ = fs::remove_dir_all(&iconset_dir);
    }

    let dtd = [
        "http",
        "://",
        "www",
        ".apple.com",
        "/DTDs/PropertyList-1.0.dtd",
    ]
    .join("");
    let info_plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"{dtd}\">
<plist version=\"1.0\"><dict><key>CFBundleIdentifier</key><string>com.contextbraid.app</string><key>CFBundleName</key><string>ContextBraid</string><key>CFBundleDisplayName</key><string>ContextBraid</string><key>CFBundleExecutable</key><string>braid</string><key>CFBundleIconFile</key><string>AppIcon</string><key>LSUIElement</key><true/></dict></plist>"
    );
    fs::write(app_contents.join("Info.plist"), info_plist)?;

    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if Path::new(lsregister).exists() {
        let _ = Command::new(lsregister)
            .args(["-f", &app_dir.to_string_lossy()])
            .output();
    }

    let home = dirs::home_dir().context("Home dir introuvable")?;
    register_native_messaging_hosts(&app_support, &app_dir, &host_link, extra_ext_ids)?;
    install_launchd_agent(&home, &app_exec)?;
    Ok(())
}

fn register_native_messaging_hosts(
    app_support: &Path,
    app_dir: &Path,
    host_link: &Path,
    extra_ext_ids: &[String],
) -> Result<()> {
    let browser_dirs = [
        "Google/Chrome/NativeMessagingHosts",
        "BraveSoftware/Brave-Browser/NativeMessagingHosts",
        "Microsoft Edge/NativeMessagingHosts",
    ];

    let mut origins = BTreeSet::new();
    origins.insert("chrome-extension://dglmcdpnglffnfdnhaeaifholhncglgc/".to_string());

    for rel in &browser_dirs {
        collect_existing_origins(
            &app_support.join(rel).join("com.contextbraid.host.json"),
            &mut origins,
        );
    }

    for id in extra_ext_ids {
        let clean = id.trim().trim_matches('/');
        if !clean.is_empty() {
            origins.insert(format!("chrome-extension://{clean}/"));
        }
    }

    let manifest_json = serde_json::json!({
        "name": "com.contextbraid.host",
        "description": "ContextBraid Native Messaging Host",
        "path": host_link.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": origins.into_iter().collect::<Vec<_>>(),
    });
    let chromium_manifest = serde_json::to_string_pretty(&manifest_json)?;

    for rel in &browser_dirs {
        let dir = app_support.join(rel);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("com.contextbraid.host.json"), &chromium_manifest)?;
    }

    let ff_dir = app_support.join("Mozilla/NativeMessagingHosts");
    fs::create_dir_all(&ff_dir)?;
    let ff_manifest = serde_json::json!({
        "name": "com.contextbraid.host",
        "description": "ContextBraid Native Messaging Host",
        "path": host_link.to_string_lossy(),
        "type": "stdio",
        "allowed_extensions": ["contextbraid@extensions.local"],
    });
    fs::write(
        ff_dir.join("com.contextbraid.host.json"),
        serde_json::to_string_pretty(&ff_manifest)?,
    )?;

    let _ = std::process::Command::new("codesign")
        .args([
            "--force",
            "--deep",
            "--sign",
            "-",
            &app_dir.to_string_lossy(),
        ])
        .status();
    let lsreg = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if Path::new(lsreg).exists() {
        let _ = std::process::Command::new(lsreg)
            .args(["-f", &app_dir.to_string_lossy()])
            .status();
    }

    Ok(())
}

fn install_launchd_agent(home: &Path, app_exec: &Path) -> Result<()> {
    let launch_agents = home.join("Library/LaunchAgents");
    fs::create_dir_all(&launch_agents)?;
    let plist_path = launch_agents.join("com.contextbraid.daemon.plist");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.contextbraid.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>daemon</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/braidd.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/braidd.err.log</string>
</dict>
</plist>
"#,
        app_exec.display()
    );
    fs::write(&plist_path, plist)?;

    let uid_out = Command::new("id").arg("-u").output()?;
    let uid = String::from_utf8_lossy(&uid_out.stdout).trim().to_string();
    let domain_target = format!("gui/{uid}");
    let service_target = format!("gui/{uid}/com.contextbraid.daemon");

    let kick = Command::new("launchctl")
        .args(["kickstart", "-k", &service_target])
        .output();
    if kick.map_or(true, |o| !o.status.success()) {
        let _ = Command::new("launchctl")
            .args(["bootstrap", &domain_target, &plist_path.to_string_lossy()])
            .output();
    }

    println!(
        "ContextBraid installé et démarré depuis {}",
        app_exec.display()
    );
    Ok(())
}
