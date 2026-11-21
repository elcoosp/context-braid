use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Detects open project directories under `repos_root` held by Zed in < 5 ms via lsof
pub fn list_zed_windows(repos_root: &Path) -> Vec<PathBuf> {
    let mut open_paths = Vec::new();
    let pids_out = match Command::new("pgrep").args(["-x", "zed"]).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => match Command::new("pgrep").args(["-x", "Zed"]).output() {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            _ => return open_paths,
        },
    };

    let pid_list: Vec<&str> = pids_out.split_whitespace().collect();
    if pid_list.is_empty() {
        return open_paths;
    }

    if let Ok(out) = Command::new("lsof")
        .args(["-a", "-p", &pid_list.join(","), "-Fn"])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(raw_path) = line.strip_prefix('n') {
                let p = Path::new(raw_path.trim());
                if p.starts_with(repos_root) && p != repos_root {
                    let pb = p.to_path_buf();
                    if !open_paths.contains(&pb) {
                        open_paths.push(pb);
                    }
                }
            }
        }
    }

    open_paths
}

pub fn get_frontmost_window() -> Option<(String, String)> {
    let script = r#"
    tell application "System Events"
        set frontApp to first application process whose frontmost is true
        set appName to name of frontApp
        set winTitle to ""
        try
            set winTitle to name of front window of frontApp
        end try
        return appName & "|||" & winTitle
    end tell
    "#;
    let out = run_osascript(script).ok()?;
    let (app, title) = out.split_once("|||")?;
    Some((app.trim().to_lowercase(), title.trim().to_string()))
}

pub fn activate_app_window(browser: &str, title_hint: Option<&str>) -> Result<()> {
    let app_name = match browser {
        "zed" => "Zed",
        "brave" => "Brave Browser",
        "chrome" => "Google Chrome",
        "edge" => "Microsoft Edge",
        "firefox" => "Firefox",
        other => other,
    };

    let hint = title_hint.unwrap_or("").replace('"', "\\\"");
    let script = if hint.is_empty() {
        format!(
            r#"
            tell application "{app}" to activate
            tell application "System Events"
                tell process "{app}"
                    repeat with w in windows
                        try
                            if value of attribute "AXMinimized" of w is true then
                                set value of attribute "AXMinimized" of w to false
                            end if
                        end try
                    end repeat
                end tell
            end tell
            "#,
            app = app_name
        )
    } else {
        format!(
            r#"
            tell application "{app}" to activate
            tell application "System Events"
                tell process "{app}"
                    set targetWin to missing value
                    repeat with w in windows
                        try
                            if (name of w) contains "{hint}" then
                                set targetWin to w
                                exit repeat
                            end if
                        end try
                    end repeat
                    if targetWin is missing value and (count of windows) > 0 then
                        set targetWin to front window
                    end if
                    if targetWin is not missing value then
                        try
                            if value of attribute "AXMinimized" of targetWin is true then
                                set value of attribute "AXMinimized" of targetWin to false
                            end if
                        end try
                        perform action "AXRaise" of targetWin
                    end if
                end tell
            end tell
            "#,
            app = app_name,
            hint = hint
        )
    };

    let _ = run_osascript(&script);
    Ok(())
}

fn run_osascript(script: &str) -> Result<String> {
    let out = Command::new("osascript").arg("-e").arg(script).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
