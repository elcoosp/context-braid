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
        let list = CGWindowListCopyWindowInfo(17, 0);
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
        "cursor" => return list_vscode_family_windows(repos_root, "Cursor"),
        "vscode" => return list_vscode_family_windows(repos_root, "Code"),
        "windsurf" => return list_vscode_family_windows(repos_root, "Windsurf"),
        _ => {}
    }

    // Fallback via CoreGraphics window titles for other editors
    let mut win_titles = list_window_titles_for_pids(&pids);
    for &pid in &pids {
        for t in ax_all_window_titles(pid) {
            if !win_titles.contains(&t) {
                win_titles.push(t);
            }
        }
    }
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

    if let Some(bin) = cli_bin
        && Path::new(bin).exists() {
            let status = std::process::Command::new(bin)
                .arg("-n")
                .arg(repo_path)
                .status()?;
            if status.success() {
                return Ok(());
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

type SparklineEntry = (String, i64, Vec<u32>);
type SparklineCacheMap = HashMap<String, SparklineEntry>;
type MergedPrEntry = (String, Vec<braid_core::ipc::MergedPr>);
type MergedPrCacheMap = HashMap<String, MergedPrEntry>;

fn sparkline_cache() -> &'static Mutex<SparklineCacheMap> {
    static CACHE: OnceLock<Mutex<SparklineCacheMap>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pr_cache() -> &'static Mutex<MergedPrCacheMap> {
    static CACHE: OnceLock<Mutex<MergedPrCacheMap>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn pr_inflight() -> &'static Mutex<HashSet<String>> {
    static INFLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    INFLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

fn trigger_async_pr_fetch(repo_path: PathBuf, head_sha: String) {
    let key = format!("{}@{}", repo_path.display(), head_sha);
    if let Ok(mut set) = pr_inflight().lock()
        && !set.insert(key.clone()) {
            return;
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
            && out.status.success()
                && let Ok(prs) = serde_json::from_slice::<Vec<braid_core::ipc::MergedPr>>(&out.stdout) {
                    fetched = prs;
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

#[cfg(test)]
static TEST_NOTIFICATIONS: std::sync::OnceLock<std::sync::Mutex<Vec<(String, String, String)>>> = std::sync::OnceLock::new();

#[cfg(test)]
pub fn take_test_notifications() -> Vec<(String, String, String)> {
    let lock = TEST_NOTIFICATIONS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut guard = lock.lock().unwrap();
    std::mem::take(&mut *guard)
}

pub fn notify_agent_alert(repo_name: &str, _browser: &str, _window_id: i64, _tab_id: i64, subtitle: &str, title: &str) {
    let msg = if title.trim().is_empty() {
        "Agent needs attention"
    } else {
        title.trim()
    };

    #[cfg(test)]
    {
        let lock = TEST_NOTIFICATIONS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
        lock.lock().unwrap().push((repo_name.to_string(), subtitle.to_string(), msg.to_string()));
    }

    #[cfg(not(test))]
    {
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
                ])
                .spawn();
        }
    }
}

pub fn deliver_un_notification_gui(repo_name: &str, subtitle: &str, message: &str) {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{NSDate, NSError, NSRunLoop, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNUserNotificationCenter,
    };
    use std::cell::Cell;
    use std::io::Write;
    use std::rc::Rc;

    let full_title = format!("ContextBraid • {}", repo_name);
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/braid-notif.log")
        .ok();

    if let Some(mtm) = MainThreadMarker::new() {
        let _app = NSApplication::sharedApplication(mtm);
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let done = Rc::new(Cell::new(false));
        let granted_flag = Rc::new(Cell::new(false));
        let done_c = Rc::clone(&done);
        let granted_c = Rc::clone(&granted_flag);

        let auth_block = RcBlock::new(move |granted: Bool, err: *mut NSError| {
            granted_c.set(granted.as_bool());
            done_c.set(true);
            if !err.is_null() {
                let desc = unsafe { (&*err).localizedDescription().to_string() };
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/braid-notif.log") {
                    let _ = writeln!(f, "AUTH_ERR: {}", desc);
                }
            }
        });

        let opts = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge;
        center.requestAuthorizationWithOptions_completionHandler(opts, &auth_block);

        let run_loop = NSRunLoop::currentRunLoop();
        for _ in 0..30 {
            if done.get() {
                break;
            }
            let next = NSDate::dateWithTimeIntervalSinceNow(0.05);
            run_loop.runUntilDate(&next);
        }

        if let Some(f) = log.as_mut() {
            let _ = writeln!(f, "AUTH_RESULT: done={} granted={}", done.get(), granted_flag.get());
        }

        if granted_flag.get() {
            let content: Retained<UNMutableNotificationContent> = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&full_title));
            content.setSubtitle(&NSString::from_str(subtitle));
            content.setBody(&NSString::from_str(message));

            let req_id = format!(
                "cb-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
            );
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&req_id),
                &content,
                None,
            );

            let sent = Rc::new(Cell::new(false));
            let ok_flag = Rc::new(Cell::new(false));
            let sent_c = Rc::clone(&sent);
            let ok_c = Rc::clone(&ok_flag);
            let add_block = RcBlock::new(move |err: *mut NSError| {
                ok_c.set(err.is_null());
                sent_c.set(true);
                if !err.is_null() {
                    let desc = unsafe { (&*err).localizedDescription().to_string() };
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/braid-notif.log") {
                        let _ = writeln!(f, "ADD_ERR: {}", desc);
                    }
                }
            });
            center.addNotificationRequest_withCompletionHandler(&request, Some(&add_block));

            for _ in 0..20 {
                if sent.get() {
                    break;
                }
                let next = NSDate::dateWithTimeIntervalSinceNow(0.05);
                run_loop.runUntilDate(&next);
            }

            if ok_flag.get() {
                // Laisse 250ms à usernoted pour récupérer l'icône du bundle avant la sortie du processus
                let next = NSDate::dateWithTimeIntervalSinceNow(0.25);
                run_loop.runUntilDate(&next);
                return;
            }
        }
    }

    // Fallback garanti si l'utilisateur n'a pas encore cliqué Autoriser
    let script = format!(
        "display notification {:?} with title {:?} subtitle {:?}",
        message, full_title, subtitle
    );
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .spawn();
}


pub fn frontmost_app_and_window() -> (String, String) {
    use objc2_core_foundation::{
        CFArray, CFDictionary, CFNumber, CFNumberType, CFRetained, CFString,
    };
    use std::ffi::c_void;

    let raw_list = unsafe { CGWindowListCopyWindowInfo(17, 0) };
    if raw_list.is_null() {
        return (String::new(), String::new());
    }
    let list: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(raw_list as *mut CFArray))
    };

    let k_layer = CFString::from_static_str("kCGWindowLayer");
    let k_owner = CFString::from_static_str("kCGWindowOwnerName");
    let k_name = CFString::from_static_str("kCGWindowName");
    let k_pid = CFString::from_static_str("kCGWindowOwnerPID");

    let count = list.len();
    for i in 0..count {
        let ptr = unsafe { list.value_at_index(i as isize) };
        if ptr.is_null() {
            continue;
        }
        let dict = unsafe { &*(ptr as *const CFDictionary) };

        let layer_ptr = unsafe { dict.value(k_layer.as_ref() as *const CFString as *const c_void) };
        let mut layer: i64 = -1;
        if !layer_ptr.is_null() {
            let num = unsafe { &*(layer_ptr as *const CFNumber) };
            unsafe { num.value(CFNumberType::SInt64Type, &mut layer as *mut i64 as *mut c_void) };
        }
        if layer != 0 {
            continue;
        }

        let owner_ptr = unsafe { dict.value(k_owner.as_ref() as *const CFString as *const c_void) };
        let owner = if !owner_ptr.is_null() {
            unsafe { &*(owner_ptr as *const CFString) }.to_string()
        } else {
            String::new()
        };
        let owner_low = owner.to_lowercase();
        if owner_low.is_empty()
            || owner_low.contains("raycast")
            || owner_low == "window server"
            || owner_low == "dock"
        {
            continue;
        }

        let mut win_name = String::new();
        let name_ptr = unsafe { dict.value(k_name.as_ref() as *const CFString as *const c_void) };
        if !name_ptr.is_null() {
            win_name = unsafe { &*(name_ptr as *const CFString) }.to_string();
        }

        if win_name.is_empty() {
            let pid_ptr = unsafe { dict.value(k_pid.as_ref() as *const CFString as *const c_void) };
            let mut pid: i32 = 0;
            if !pid_ptr.is_null() {
                let num = unsafe { &*(pid_ptr as *const CFNumber) };
                unsafe { num.value(CFNumberType::SInt32Type, &mut pid as *mut i32 as *mut c_void) };
            }
            if pid > 0 {
                win_name = ax_focused_window_title(pid).unwrap_or_default();
            }
        }

        return (owner, win_name);
    }

    (String::new(), String::new())
}

fn ax_focused_window_title(pid: i32) -> Option<String> {
    use objc2_core_foundation::{CFRetained, CFString};
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
        fn AXUIElementCopyAttributeValue(
            element: *const c_void,
            attribute: *const CFString,
            value: *mut *const c_void,
        ) -> i32;
        fn CFRelease(cf: *const c_void);
    }

    let app_el = unsafe { AXUIElementCreateApplication(pid) };
    if app_el.is_null() {
        return None;
    }

    let attr_focused = CFString::from_static_str("AXFocusedWindow");
    let mut win_el: *const c_void = std::ptr::null();
    let err1 = unsafe {
        AXUIElementCopyAttributeValue(app_el, attr_focused.as_ref(), &mut win_el)
    };
    unsafe { CFRelease(app_el) };
    if err1 != 0 || win_el.is_null() {
        return None;
    }

    let attr_title = CFString::from_static_str("AXTitle");
    let mut title_cf: *const c_void = std::ptr::null();
    let err2 = unsafe {
        AXUIElementCopyAttributeValue(win_el, attr_title.as_ref(), &mut title_cf)
    };
    unsafe { CFRelease(win_el) };
    if err2 != 0 || title_cf.is_null() {
        return None;
    }

    let title_str: CFRetained<CFString> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(title_cf as *mut CFString))
    };
    let s = title_str.to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub fn focus_specific_editor_window(favorite: &str, repo_name: &str) -> anyhow::Result<()> {
    use objc2_core_foundation::{CFArray, CFRetained, CFString};
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
        fn AXUIElementCopyAttributeValue(
            element: *const c_void,
            attribute: *const CFString,
            value: *mut *const c_void,
        ) -> i32;
        fn AXUIElementPerformAction(element: *const c_void, action: *const CFString) -> i32;
        fn CFRelease(cf: *const c_void);
    }

    focus_editor_app(favorite)?;
    let spec = resolve_editor_spec(favorite);

    let pid = find_running_app_pid(spec.bundle_id).unwrap_or(0);
    if pid <= 0 {
        return Ok(());
    }

    let app_el = unsafe { AXUIElementCreateApplication(pid) };
    if app_el.is_null() {
        return Ok(());
    }

    let attr_windows = CFString::from_static_str("AXWindows");
    let mut wins_cf: *const c_void = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(app_el, attr_windows.as_ref(), &mut wins_cf) };
    unsafe { CFRelease(app_el) };
    if err != 0 || wins_cf.is_null() {
        return Ok(());
    }

    let wins: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(wins_cf as *mut CFArray))
    };
    let attr_title = CFString::from_static_str("AXTitle");
    let act_raise = CFString::from_static_str("AXRaise");
    let target_low = repo_name.to_lowercase();

    for i in 0..wins.len() {
        let w_ptr = unsafe { wins.value_at_index(i as isize) };
        if w_ptr.is_null() {
            continue;
        }
        let mut title_cf: *const c_void = std::ptr::null();
        let t_err = unsafe { AXUIElementCopyAttributeValue(w_ptr, attr_title.as_ref(), &mut title_cf) };
        if t_err == 0 && !title_cf.is_null() {
            let title_str: CFRetained<CFString> = unsafe {
                CFRetained::from_raw(std::ptr::NonNull::new_unchecked(title_cf as *mut CFString))
            };
            if title_str.to_string().to_lowercase().contains(&target_low) {
                unsafe { AXUIElementPerformAction(w_ptr, act_raise.as_ref()) };
                break;
            }
        }
    }

    Ok(())
}

fn find_running_app_pid(bundle_id: &str) -> Option<i32> {
    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    let ns_bid = NSString::from_str(bundle_id);
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&ns_bid);
    apps.firstObject().map(|app| app.processIdentifier())
}

fn ax_all_window_titles(pid: i32) -> Vec<String> {
    use objc2_core_foundation::{CFArray, CFRetained, CFString};
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
        fn AXUIElementCopyAttributeValue(
            element: *const c_void,
            attribute: *const CFString,
            value: *mut *const c_void,
        ) -> i32;
        fn CFRelease(cf: *const c_void);
    }

    let mut out = Vec::new();
    let app_el = unsafe { AXUIElementCreateApplication(pid) };
    if app_el.is_null() {
        return out;
    }

    let attr_windows = CFString::from_static_str("AXWindows");
    let mut wins_cf: *const c_void = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(app_el, attr_windows.as_ref(), &mut wins_cf) };
    unsafe { CFRelease(app_el) };
    if err != 0 || wins_cf.is_null() {
        return out;
    }

    let wins: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(wins_cf as *mut CFArray))
    };
    let attr_title = CFString::from_static_str("AXTitle");
    for i in 0..wins.len() {
        let w_ptr = unsafe { wins.value_at_index(i as isize) };
        if w_ptr.is_null() {
            continue;
        }
        let mut title_cf: *const c_void = std::ptr::null();
        let t_err = unsafe { AXUIElementCopyAttributeValue(w_ptr, attr_title.as_ref(), &mut title_cf) };
        if t_err == 0 && !title_cf.is_null() {
            let title_str: CFRetained<CFString> = unsafe {
                CFRetained::from_raw(std::ptr::NonNull::new_unchecked(title_cf as *mut CFString))
            };
            let s = title_str.to_string().trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}
