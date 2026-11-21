use anyhow::Result;
use std::ffi::{c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    fn objc_getClass(name: *const i8) -> *const c_void;
    fn sel_registerName(name: *const i8) -> *const c_void;
    fn objc_msgSend();
}

unsafe fn activate_bundle_id(bundle_id: &str) -> bool {
    let cls_ns_running = unsafe { objc_getClass(c"NSRunningApplication".as_ptr()) };
    let cls_ns_str = unsafe { objc_getClass(c"NSString".as_ptr()) };
    if cls_ns_running.is_null() || cls_ns_str.is_null() {
        return false;
    }

    let sel_str_utf8 = unsafe { sel_registerName(c"stringWithUTF8String:".as_ptr()) };
    let sel_running_by_id = unsafe { sel_registerName(c"runningApplicationsWithBundleIdentifier:".as_ptr()) };
    let sel_count = unsafe { sel_registerName(c"count".as_ptr()) };
    let sel_obj_at = unsafe { sel_registerName(c"objectAtIndex:".as_ptr()) };
    let sel_activate = unsafe { sel_registerName(c"activateWithOptions:".as_ptr()) };

    let c_bundle = CString::new(bundle_id).unwrap();

    type MsgStr = unsafe extern "C" fn(*const c_void, *const c_void, *const i8) -> *const c_void;
    type MsgArr = unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void;
    type MsgCount = unsafe extern "C" fn(*const c_void, *const c_void) -> usize;
    type MsgAt = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> *const c_void;
    type MsgAct = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> bool;

    let fn_str: MsgStr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_arr: MsgArr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_count: MsgCount = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_at: MsgAt = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_act: MsgAct = unsafe { std::mem::transmute(objc_msgSend as *const ()) };

    let ns_bundle = unsafe { fn_str(cls_ns_str, sel_str_utf8, c_bundle.as_ptr()) };
    let apps = unsafe { fn_arr(cls_ns_running, sel_running_by_id, ns_bundle) };
    if apps.is_null() {
        return false;
    }
    let count = unsafe { fn_count(apps, sel_count) };
    if count == 0 {
        return false;
    }
    let app = unsafe { fn_at(apps, sel_obj_at, 0) };
    // NSApplicationActivateIgnoringOtherApps = 1 << 1 = 2
    unsafe { fn_act(app, sel_activate, 2) }
}

pub fn frontmost_bundle_id() -> Option<String> {
    let cls_ws = unsafe { objc_getClass(c"NSWorkspace".as_ptr()) };
    if cls_ws.is_null() {
        return None;
    }
    let sel_shared = unsafe { sel_registerName(c"sharedWorkspace".as_ptr()) };
    let sel_front = unsafe { sel_registerName(c"frontmostApplication".as_ptr()) };
    let sel_bundle = unsafe { sel_registerName(c"bundleIdentifier".as_ptr()) };
    let sel_utf8 = unsafe { sel_registerName(c"UTF8String".as_ptr()) };

    type MsgObj = unsafe extern "C" fn(*const c_void, *const c_void) -> *const c_void;
    type MsgCStr = unsafe extern "C" fn(*const c_void, *const c_void) -> *const i8;

    let fn_obj: MsgObj = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_cstr: MsgCStr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };

    let ws = unsafe { fn_obj(cls_ws, sel_shared) };
    let app = unsafe { fn_obj(ws, sel_front) };
    if app.is_null() {
        return None;
    }
    let ns_id = unsafe { fn_obj(app, sel_bundle) };
    if ns_id.is_null() {
        return None;
    }
    let c_ptr = unsafe { fn_cstr(ns_id, sel_utf8) };
    if c_ptr.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(c_ptr) }.to_string_lossy().into_owned())
}

pub struct EditorSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub bundle_id: &'static str,
    pub cli_app: &'static str,
}

pub fn resolve_editor_spec(favorite: &str) -> EditorSpec {
    match favorite.trim().to_lowercase().as_str() {
        "cursor" => EditorSpec {
            id: "cursor",
            label: "Cursor",
            bundle_id: "com.todesktop.230313mzl4w4u92",
            cli_app: "Cursor",
        },
        "vscode" | "code" | "visual studio code" => EditorSpec {
            id: "vscode",
            label: "VS Code",
            bundle_id: "com.microsoft.VSCode",
            cli_app: "Visual Studio Code",
        },
        "windsurf" => EditorSpec {
            id: "windsurf",
            label: "Windsurf",
            bundle_id: "com.exafunction.windsurf",
            cli_app: "Windsurf",
        },
        "neovim" | "nvim" => EditorSpec {
            id: "neovim",
            label: "Neovim",
            bundle_id: "com.mitchellh.ghostty",
            cli_app: "Ghostty",
        },
        _ => EditorSpec {
            id: "zed",
            label: "Zed",
            bundle_id: "dev.zed.Zed",
            cli_app: "Zed",
        },
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFArrayGetCount(the_array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(the_array: *const c_void, idx: isize) -> *const c_void;
    fn CFDictionaryGetValue(the_dict: *const c_void, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(alloc: *const c_void, c_str: *const i8, encoding: u32) -> *const c_void;
    fn CFStringGetCString(the_string: *const c_void, buffer: *mut i8, buffer_size: isize, encoding: u32) -> bool;
    fn CFNumberGetValue(number: *const c_void, the_type: isize, value_ptr: *mut c_void) -> bool;
}

fn running_pids_for_bundle(bundle_id: &str) -> Vec<i32> {
    let mut pids = Vec::new();
    unsafe {
        let cls_ns_running = objc_getClass(c"NSRunningApplication".as_ptr());
        let cls_ns_str = objc_getClass(c"NSString".as_ptr());
        if cls_ns_running.is_null() || cls_ns_str.is_null() {
            return pids;
        }
        let sel_str_utf8 = sel_registerName(c"stringWithUTF8String:".as_ptr());
        let sel_running_by_id = sel_registerName(c"runningApplicationsWithBundleIdentifier:".as_ptr());
        let sel_count = sel_registerName(c"count".as_ptr());
        let sel_obj_at = sel_registerName(c"objectAtIndex:".as_ptr());
        let sel_pid = sel_registerName(c"processIdentifier".as_ptr());

        type MsgStr = unsafe extern "C" fn(*const c_void, *const c_void, *const i8) -> *const c_void;
        type MsgArr = unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void;
        type MsgCount = unsafe extern "C" fn(*const c_void, *const c_void) -> usize;
        type MsgAt = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> *const c_void;
        type MsgPid = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;

        let fn_str: MsgStr = std::mem::transmute(objc_msgSend as *const ());
        let fn_arr: MsgArr = std::mem::transmute(objc_msgSend as *const ());
        let fn_count: MsgCount = std::mem::transmute(objc_msgSend as *const ());
        let fn_at: MsgAt = std::mem::transmute(objc_msgSend as *const ());
        let fn_pid: MsgPid = std::mem::transmute(objc_msgSend as *const ());

        let c_bundle = CString::new(bundle_id).unwrap();
        let ns_bundle = fn_str(cls_ns_str, sel_str_utf8, c_bundle.as_ptr());
        let apps = fn_arr(cls_ns_running, sel_running_by_id, ns_bundle);
        if apps.is_null() {
            return pids;
        }
        let count = fn_count(apps, sel_count);
        for i in 0..count {
            let app = fn_at(apps, sel_obj_at, i);
            let pid = fn_pid(app, sel_pid);
            if pid > 0 {
                pids.push(pid);
            }
        }
    }
    pids
}

fn list_window_titles_for_pids(target_pids: &[i32]) -> Vec<String> {
    let mut titles = Vec::new();
    if target_pids.is_empty() {
        return titles;
    }
    unsafe {
        // kCGWindowListOptionAll = 0, kCGWindowListExcludeDesktopElements = 16
        let list = CGWindowListCopyWindowInfo(16, 0);
        if list.is_null() {
            return titles;
        }
        // kCFStringEncodingUTF8 = 0x08000100
        let k_pid = CFStringCreateWithCString(std::ptr::null(), c"kCGWindowOwnerPID".as_ptr(), 0x08000100);
        let k_name = CFStringCreateWithCString(std::ptr::null(), c"kCGWindowName".as_ptr(), 0x08000100);
        let k_layer = CFStringCreateWithCString(std::ptr::null(), c"kCGWindowLayer".as_ptr(), 0x08000100);

        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() {
                continue;
            }
            let pid_ref = CFDictionaryGetValue(dict, k_pid);
            let mut pid: i32 = 0;
            // kCFNumberSInt32Type = 3
            if pid_ref.is_null() || !CFNumberGetValue(pid_ref, 3, &mut pid as *mut i32 as *mut c_void) {
                continue;
            }
            if !target_pids.contains(&pid) {
                continue;
            }
            let layer_ref = CFDictionaryGetValue(dict, k_layer);
            let mut layer: i32 = 0;
            if !layer_ref.is_null() {
                CFNumberGetValue(layer_ref, 3, &mut layer as *mut i32 as *mut c_void);
            }
            if layer != 0 {
                continue;
            }
            let name_ref = CFDictionaryGetValue(dict, k_name);
            if !name_ref.is_null() {
                let mut buf = [0i8; 512];
                if CFStringGetCString(name_ref, buf.as_mut_ptr(), buf.len() as isize, 0x08000100) {
                    let s = CStr::from_ptr(buf.as_ptr()).to_string_lossy().trim().to_string();
                    if !s.is_empty() {
                        titles.push(s);
                    }
                }
            }
        }

        CFRelease(k_pid);
        CFRelease(k_name);
        CFRelease(k_layer);
        CFRelease(list);
    }
    titles
}

fn list_zed_active_session_paths(repos_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return out;
    };
    let db_path = home
        .join("Library/Application Support/Zed/db/0-stable/db.sqlite");
    if !db_path.exists() {
        return out;
    }
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, flags) else {
        return out;
    };

    // Only select workspaces belonging to the live Zed process's current session_id
    let Ok(mut stmt) = conn.prepare(
        "SELECT paths FROM workspaces
         WHERE session_id = (
             SELECT session_id FROM workspaces
             WHERE session_id IS NOT NULL AND session_id != ''
             ORDER BY timestamp DESC LIMIT 1
         )"
    ) else {
        return out;
    };
    let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
        return out;
    };
    for paths_str in rows.flatten() {
        for raw_line in paths_str.lines() {
            let p = Path::new(raw_line.trim());
            if p.starts_with(repos_root) && p != repos_root {
                let pb = p.to_path_buf();
                if !out.contains(&pb) {
                    out.push(pb);
                }
            }
        }
    }
    out
}

fn list_vscode_family_windows(repos_root: &Path, app_folder: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return out;
    };
    let storage_path = home
        .join("Library/Application Support")
        .join(app_folder)
        .join("User/globalStorage/storage.json");
    let Ok(raw) = std::fs::read_to_string(&storage_path) else {
        return out;
    };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return out;
    };
    let Some(opened) = val
        .pointer("/windowsState/openedWindows")
        .and_then(|v| v.as_array())
    else {
        return out;
    };
    for win in opened {
        let Some(folder_uri) = win.get("folder").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(raw_path) = folder_uri.strip_prefix("file://") {
            let decoded = raw_path.replace("%20", " ");
            let p = Path::new(&decoded);
            if p.starts_with(repos_root) && p != repos_root {
                let pb = p.to_path_buf();
                if !out.contains(&pb) {
                    out.push(pb);
                }
            }
        }
    }
    out
}

pub fn list_editor_windows(repos_root: &Path, favorite: &str) -> Vec<PathBuf> {
    let spec = resolve_editor_spec(favorite);
    let pids = running_pids_for_bundle(spec.bundle_id);
    if pids.is_empty() {
        return Vec::new();
    }

    match spec.id {
        "zed" => return list_zed_active_session_paths(repos_root),
        "cursor" => return list_vscode_family_windows(repos_root, "Cursor"),
        "vscode" => return list_vscode_family_windows(repos_root, "Code"),
        "windsurf" => return list_vscode_family_windows(repos_root, "Windsurf"),
        _ => {}
    }

    // Fallback via CoreGraphics window titles for other editors
    let win_titles = list_window_titles_for_pids(&pids);
    let mut open_paths = Vec::new();
    let Ok(entries) = std::fs::read_dir(repos_root) else {
        return open_paths;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        let matched = win_titles.iter().any(|t| {
            let tl = t.to_lowercase();
            tl == name
                || tl.starts_with(&format!("{} ", name))
                || tl.starts_with(&format!("{} —", name))
                || tl.starts_with(&format!("{} -", name))
                || tl.ends_with(&format!(" — {}", name))
                || tl.ends_with(&format!(" - {}", name))
        });
        if matched {
            open_paths.push(path);
        }
    }
    open_paths
}

pub fn focus_editor_app(favorite: &str) -> Result<()> {
    let spec = resolve_editor_spec(favorite);
    unsafe {
        activate_bundle_id(spec.bundle_id);
    }
    Ok(())
}

pub fn open_in_favorite_editor(favorite: &str, repo_path: impl AsRef<Path>) -> Result<()> {
    let repo_path = repo_path.as_ref();
    let spec = resolve_editor_spec(favorite);
    let cli_bin = match spec.id {
        "zed" => Some("/Applications/Zed.app/Contents/MacOS/cli"),
        "cursor" => Some("/Applications/Cursor.app/Contents/Resources/app/bin/cursor"),
        "vscode" => Some("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"),
        "windsurf" => Some("/Applications/Windsurf.app/Contents/Resources/app/bin/windsurf"),
        _ => None,
    };

    if let Some(bin) = cli_bin {
        if Path::new(bin).exists() {
            let status = std::process::Command::new(bin)
                .arg("-n")
                .arg(repo_path)
                .status()?;
            if status.success() {
                return Ok(());
            }
        }
    }

    let status = std::process::Command::new("open")
        .args(["-na", spec.cli_app, "--args", "-n"])
        .arg(repo_path)
        .status()?;
    if !status.success() {
        anyhow::bail!("Failed to open {} in new window", spec.label);
    }
    Ok(())
}

pub fn open_in_terminal(repo_path: &str) -> Result<()> {
    let _ = Command::new("open").args(["-a", "Terminal", repo_path]).output();
    Ok(())
}

pub fn activate_browser_app(browser: &str) {
    let bundle = match browser {
        "brave" => "com.brave.Browser",
        "chrome" => "com.google.Chrome",
        "edge" => "com.microsoft.edgemac",
        "firefox" => "org.mozilla.firefox",
        _ => return,
    };
    unsafe {
        activate_bundle_id(bundle);
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct GitRepoMeta {
    pub branch: String,
    pub dirty: usize,
    pub commit_sha: String,
    pub commit_msg: String,
    pub commit_ts: i64,
    pub sparkline: Vec<u32>,
    pub sparkline_png_path: String,
    pub repo_url: String,
    pub recent_prs: Vec<braid_core::ipc::MergedPr>,
}

fn sparkline_cache() -> &'static Mutex<HashMap<String, (String, i64, Vec<u32>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, i64, Vec<u32>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pr_cache() -> &'static Mutex<HashMap<String, (String, Vec<braid_core::ipc::MergedPr>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, Vec<braid_core::ipc::MergedPr>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pr_inflight() -> &'static Mutex<HashSet<String>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn trigger_async_pr_fetch(repo_path: PathBuf, head_sha: String) {
    let key = format!("{}@{}", repo_path.display(), head_sha);
    if let Ok(mut set) = pr_inflight().lock() {
        if !set.insert(key.clone()) {
            return;
        }
    }

    std::thread::spawn(move || {
        let mut fetched = Vec::new();
        let gh_bin = if Path::new("/opt/homebrew/bin/gh").exists() {
            "/opt/homebrew/bin/gh"
        } else if Path::new("/usr/local/bin/gh").exists() {
            "/usr/local/bin/gh"
        } else {
            "gh"
        };

        if let Ok(out) = std::process::Command::new(gh_bin)
            .current_dir(&repo_path)
            .args(["pr", "list", "--state", "merged", "--limit", "3", "--json", "number,title,url"])
            .output()
        {
            if out.status.success() {
                if let Ok(prs) = serde_json::from_slice::<Vec<braid_core::ipc::MergedPr>>(&out.stdout) {
                    fetched = prs;
                }
            }
        }

        let repo_key = repo_path.to_string_lossy().into_owned();
        if let Ok(mut cache) = pr_cache().lock() {
            cache.insert(repo_key, (head_sha, fetched));
        }
        if let Ok(mut set) = pr_inflight().lock() {
            set.remove(&key);
        }
    });
}

fn remote_to_github_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches(".git");
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return format!("https://github.com{}", rest);
    }
    if let Some(rest) = trimmed.strip_prefix("ssh://git@://github.com") {
        return format!("https://github.com{}", rest);
    }
    if trimmed.starts_with("https://github.com") || trimmed.starts_with("http://://github.com") {
        return trimmed.replace("http://", "https://");
    }
    String::new()
}

pub fn git_repo_stats(repo_path: &Path) -> GitRepoMeta {
    let Ok(repo) = gix::open(repo_path) else {
        return GitRepoMeta {
            branch: String::new(),
            dirty: 0,
            commit_sha: String::new(),
            commit_msg: String::new(),
            commit_ts: 0,
            sparkline: vec![0; 30],
            sparkline_png_path: String::new(),
            repo_url: String::new(),
            recent_prs: Vec::new(),
        };
    };

    let branch = repo
        .head_name()
        .ok()
        .flatten()
        .map(|n| n.shorten().to_string())
        .unwrap_or_else(|| {
            repo.head_id()
                .ok()
                .map(|id| id.to_hex_with_len(7).to_string())
                .unwrap_or_default()
        });

    let mut commit_sha = String::new();
    let mut commit_msg = String::new();
    let mut commit_ts: i64 = 0;
    let mut sparkline = vec![0u32; 30];
    let mut recent_prs = Vec::new();

    if let Ok(head_commit) = repo.head_commit() {
        commit_sha = head_commit.id().to_hex_with_len(7).to_string();
        commit_msg = head_commit
            .message()
            .ok()
            .map(|m| m.summary().to_string())
            .unwrap_or_default();
        commit_ts = head_commit.time().map(|t| t.seconds).unwrap_or(0);

        let sys_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(commit_ts);
        let now_sec = sys_now.max(commit_ts);
        let today_bucket = now_sec / 86_400;
        let cache_key = repo_path.to_string_lossy().into_owned();

        // 1. 30-day commit sparkline cached in O(1) by (HEAD SHA, day)
        if let Some((_, _, cached_spark)) = sparkline_cache()
            .lock()
            .ok()
            .and_then(|g| g.get(&cache_key).cloned())
            .filter(|(sha, day, _)| sha == &commit_sha && *day == today_bucket)
        {
            sparkline = cached_spark;
        } else {
            let cutoff = now_sec - 30 * 86_400;
            if let Ok(walk) = repo
                .rev_walk([head_commit.id()])
                .sorting(gix::revision::walk::Sorting::ByCommitTime(Default::default()))
                .all()
            {
                for info in walk.flatten().take(25000) {
                    let Some(ts) = info.commit_time else { continue; };
                    if ts < cutoff {
                        break;
                    }
                    let age_days = ((now_sec - ts).max(0) / 86_400) as usize;
                    if age_days < 30 {
                        let bucket = 29 - age_days;
                        sparkline[bucket] = sparkline[bucket].saturating_add(1);
                    }
                }
            }
            if let Ok(mut g) = sparkline_cache().lock() {
                g.insert(cache_key.clone(), (commit_sha.clone(), today_bucket, sparkline.clone()));
            }
        }

        // 2. Merged PRs (Merge, Squash, AND Rebase/reapply) via non-blocking gh CLI worker cached by HEAD SHA
        if let Some((cached_sha, cached_prs)) = pr_cache().lock().ok().and_then(|g| g.get(&cache_key).cloned()) {
            recent_prs = cached_prs;
            if cached_sha != commit_sha {
                trigger_async_pr_fetch(repo_path.to_path_buf(), commit_sha.clone());
            }
        } else {
            trigger_async_pr_fetch(repo_path.to_path_buf(), commit_sha.clone());
        }
    }

    let dirty = repo
        .status(gix::progress::Discard)
        .ok()
        .and_then(|platform| platform.into_index_worktree_iter(Vec::new()).ok())
        .map(|iter| iter.filter_map(Result::ok).count())
        .unwrap_or(0);

    let repo_url = std::fs::read_to_string(repo_path.join(".git/config"))
        .ok()
        .and_then(|cfg| {
            cfg.lines()
                .find(|l| l.trim_start().starts_with("url ="))
                .and_then(|l| l.split_once('='))
                .map(|(_, u)| remote_to_github_url(u))
        })
        .unwrap_or_default();

    let repo_label = repo_path.file_name().and_then(|s| s.to_str()).unwrap_or("repo");
    let accent = if dirty > 0 { (255, 159, 10) } else { (10, 132, 255) };
    let sparkline_png_path = crate::chart_png::render_velocity_chart_png(repo_label, &sparkline, accent).unwrap_or_default();
    GitRepoMeta {
        branch,
        dirty,
        commit_sha,
        commit_msg,
        commit_ts,
        sparkline,
        sparkline_png_path,
        repo_url,
        recent_prs,
    }
}

pub fn notify_agent_alert(repo_name: &str, browser: &str, window_id: i64, tab_id: i64, subtitle: &str, title: &str) {
    let msg = if title.trim().is_empty() {
        "Agent needs attention"
    } else {
        title.trim()
    };

    let sound_file = if subtitle.to_lowercase().contains("zip") {
        "/System/Library/Sounds/Glass.aiff"
    } else {
        "/System/Library/Sounds/Funk.aiff"
    };
    if Path::new(sound_file).exists() {
        let _ = std::process::Command::new("afplay")
            .arg(sound_file)
            .spawn();
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let app_bundle = home.join("Library/Application Support/ContextBraid/ContextBraid.app");
    if app_bundle.exists() {
        let _ = std::process::Command::new("open")
            .args([
                "-n",
                "-g",
                "-a",
                &app_bundle.to_string_lossy(),
                "--args",
                "notify",
                repo_name,
                subtitle,
                msg,
                browser,
                &window_id.to_string(),
                &tab_id.to_string(),
            ])
            .spawn();
    } else {
        deliver_gui_notification(repo_name, subtitle, msg, browser, window_id, tab_id);
    }
}

pub fn deliver_gui_notification(repo_name: &str, subtitle: &str, message: &str, browser: &str, window_id: i64, tab_id: i64) {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNUserNotificationCenter,
    };
    use std::sync::mpsc;
    use std::time::Duration;

    let full_title = format!("ContextBraid • {}", repo_name);
    let center = UNUserNotificationCenter::currentNotificationCenter();

    let (tx, rx) = mpsc::channel::<bool>();
    let auth_block = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        let _ = tx.send(granted.as_bool());
    });

    let opts = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge;
    center.requestAuthorizationWithOptions_completionHandler(opts, &auth_block);

    let granted = rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false);
    if granted {
        let content: Retained<UNMutableNotificationContent> = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&full_title));
        content.setSubtitle(&NSString::from_str(subtitle));
        content.setBody(&NSString::from_str(message));

        let req_id = format!("contextbraid-{}-{}-{}", browser, window_id, tab_id);
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&req_id),
            &content,
            None,
        );

        let (done_tx, done_rx) = mpsc::channel::<bool>();
        let add_block = RcBlock::new(move |err: *mut NSError| {
            let _ = done_tx.send(err.is_null());
        });
        center.addNotificationRequest_withCompletionHandler(&request, Some(&add_block));
        let ok = done_rx.recv_timeout(Duration::from_secs(2)).unwrap_or(false);
        if ok {
            return;
        }
    }

    // Fallback instantané si l'autorisation UNUserNotificationCenter n'est pas encore accordée
    let script = format!(
        "display notification {:?} with title {:?} subtitle {:?}",
        message, full_title, subtitle
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .spawn();
}
