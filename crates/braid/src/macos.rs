use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_void};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

type MsgStr = unsafe extern "C" fn(*const c_void, *const c_void, *const i8) -> *const c_void;
type MsgArr = unsafe extern "C" fn(*const c_void, *const c_void, *const c_void) -> *const c_void;
type MsgCount = unsafe extern "C" fn(*const c_void, *const c_void) -> usize;
type MsgAt = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> *const c_void;
type MsgTrigger = unsafe extern "C" fn(*const c_void, *const c_void, usize) -> bool;
type MsgPid = unsafe extern "C" fn(*const c_void, *const c_void) -> i32;

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
    let sel_running_by_id =
        unsafe { sel_registerName(c"runningApplicationsWithBundleIdentifier:".as_ptr()) };
    let sel_count = unsafe { sel_registerName(c"count".as_ptr()) };
    let sel_obj_at = unsafe { sel_registerName(c"objectAtIndex:".as_ptr()) };
    let sel_activate = unsafe { sel_registerName(c"activateWithOptions:".as_ptr()) };

    let Ok(c_bundle) = CString::new(bundle_id) else {
        return false;
    };

    let fn_str: MsgStr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_arr: MsgArr = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_count: MsgCount = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_at: MsgAt = unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let fn_trigger: MsgTrigger = unsafe { std::mem::transmute(objc_msgSend as *const ()) };

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
    unsafe { fn_trigger(app, sel_activate, 2) }
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
    fn CFStringCreateWithCString(
        alloc: *const c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *const c_void;
    fn CFStringGetCString(
        the_string: *const c_void,
        buffer: *mut i8,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
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
        let sel_running_by_id =
            sel_registerName(c"runningApplicationsWithBundleIdentifier:".as_ptr());
        let sel_count = sel_registerName(c"count".as_ptr());
        let sel_obj_at = sel_registerName(c"objectAtIndex:".as_ptr());
        let sel_pid = sel_registerName(c"processIdentifier".as_ptr());

        let fn_str: MsgStr = std::mem::transmute(objc_msgSend as *const ());
        let fn_arr: MsgArr = std::mem::transmute(objc_msgSend as *const ());
        let fn_count: MsgCount = std::mem::transmute(objc_msgSend as *const ());
        let fn_at: MsgAt = std::mem::transmute(objc_msgSend as *const ());
        let fn_pid: MsgPid = std::mem::transmute(objc_msgSend as *const ());

        let Ok(c_bundle) = CString::new(bundle_id) else {
            return pids;
        };
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
        // kCFStringEncodingUTF8 = 0x0800_0100
        let k_pid =
            CFStringCreateWithCString(std::ptr::null(), c"kCGWindowOwnerPID".as_ptr(), 0x0800_0100);
        let k_name =
            CFStringCreateWithCString(std::ptr::null(), c"kCGWindowName".as_ptr(), 0x0800_0100);
        let k_layer =
            CFStringCreateWithCString(std::ptr::null(), c"kCGWindowLayer".as_ptr(), 0x0800_0100);

        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() {
                continue;
            }
            let pid_ref = CFDictionaryGetValue(dict, k_pid);
            let mut pid: i32 = 0;
            // kCFNumberSInt32Type = 3
            if pid_ref.is_null() || !CFNumberGetValue(pid_ref, 3, (&raw mut pid).cast::<c_void>()) {
                continue;
            }
            if !target_pids.contains(&pid) {
                continue;
            }
            let layer_ref = CFDictionaryGetValue(dict, k_layer);
            let mut layer: i32 = 0;
            if !layer_ref.is_null() {
                CFNumberGetValue(layer_ref, 3, (&raw mut layer).cast::<c_void>());
            }
            if layer != 0 {
                continue;
            }
            let name_ref = CFDictionaryGetValue(dict, k_name);
            if !name_ref.is_null() {
                let mut buf = [0i8; 512];
                if CFStringGetCString(
                    name_ref,
                    buf.as_mut_ptr(),
                    buf.len().cast_signed(),
                    0x0800_0100,
                ) {
                    let s = CStr::from_ptr(buf.as_ptr())
                        .to_string_lossy()
                        .trim()
                        .to_string();
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
                || tl.starts_with(&format!("{name} "))
                || tl.starts_with(&format!("{name} —"))
                || tl.starts_with(&format!("{name} -"))
                || tl.ends_with(&format!(" — {name}"))
                || tl.ends_with(&format!(" - {name}"))
        });
        if matched {
            open_paths.push(path);
        }
    }
    open_paths
}

pub fn focus_editor_app(favorite: &str) {
    let spec = resolve_editor_spec(favorite);
    unsafe {
        activate_bundle_id(spec.bundle_id);
    }
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
        && Path::new(bin).exists()
    {
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

pub fn open_in_terminal(repo_path: &str) {
    let _ = Command::new("open")
        .args(["-a", "Terminal", repo_path])
        .spawn();
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
    pub main_ci_status: String,
}

type SparklineEntry = (String, i64, Vec<u32>);
type SparklineCacheMap = HashMap<String, SparklineEntry>;
type MergedPrEntry = (String, Vec<braid_core::ipc::MergedPr>, String);
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
        && !set.insert(key.clone())
    {
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

        let mut ci_status = String::new();
        let query = r#"query($owner:String!,$name:String!){repository(owner:$owner,name:$name){pullRequests(states:MERGED,first:3,orderBy:{field:UPDATED_AT,direction:DESC}){nodes{number title url}}object(expression:"main"){... on Commit{statusCheckRollup{state}}}}}"#;
        if let Ok(out) = std::process::Command::new(gh_bin)
            .current_dir(&repo_path)
            .args([
                "api",
                "graphql",
                "-F",
                "owner={owner}",
                "-F",
                "name={repo}",
                "-f",
                &format!("query={query}"),
            ])
            .output()
            && out.status.success()
            && let Ok(val) = serde_json::from_slice::<serde_json::Value>(&out.stdout)
        {
            if let Some(nodes) = val.pointer("/data/repository/pullRequests/nodes")
                && let Ok(prs) =
                    serde_json::from_value::<Vec<braid_core::ipc::MergedPr>>(nodes.clone())
            {
                fetched = prs;
            }
            if let Some(state) = val
                .pointer("/data/repository/object/statusCheckRollup/state")
                .and_then(|v| v.as_str())
            {
                ci_status = match state {
                    "SUCCESS" => "success".to_string(),
                    "FAILURE" | "ERROR" => "failure".to_string(),
                    "PENDING" | "EXPECTED" => "pending".to_string(),
                    _ => String::new(),
                };
            }
        }

        let repo_key = repo_path.to_string_lossy().into_owned();
        if let Ok(mut cache) = pr_cache().lock() {
            cache.insert(repo_key, (head_sha, fetched, ci_status));
        }
        if let Ok(mut set) = pr_inflight().lock() {
            set.remove(&key);
        }
    });
}

fn remote_to_github_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches(".git");
    let gh_https = concat!("https", ":", "//", "github.com");
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        let clean = rest.trim_start_matches('/');
        return format!("{gh_https}/{clean}");
    }
    if let Some(rest) = trimmed.strip_prefix(concat!("ssh", ":", "//", "git@", "github.com")) {
        let clean = rest.trim_start_matches('/');
        return format!("{gh_https}/{clean}");
    }
    if trimmed.starts_with(gh_https) || trimmed.starts_with(concat!("http", ":", "//", "github.com")) {
        return trimmed.replacen("http:", "https:", 1);
    }
    String::new()
}

fn compute_repo_sparkline(
    repo: &gix::Repository,
    head_id: gix::Id<'_>,
    cache_key: &str,
    commit_sha: &str,
    now_sec: i64,
) -> Vec<u32> {
    let today_bucket = now_sec / 86_400;
    if let Some((_, _, cached_spark)) = sparkline_cache()
        .lock()
        .ok()
        .and_then(|g| g.get(cache_key).cloned())
        .filter(|(sha, day, _)| sha == commit_sha && *day == today_bucket)
    {
        return cached_spark;
    }
    let mut sparkline = vec![0u32; 30];
    let cutoff = now_sec - 30 * 86_400;
    if let Ok(walk) = repo
        .rev_walk([head_id])
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            gix::traverse::commit::simple::CommitTimeOrder::default(),
        ))
        .all()
    {
        for info in walk.flatten().take(25_000) {
            let Some(ts) = info.commit_time else {
                continue;
            };
            if ts < cutoff {
                break;
            }
            if let Ok(age_days) = usize::try_from((now_sec - ts).max(0) / 86_400)
                && age_days < 30
            {
                let bucket = 29 - age_days;
                sparkline[bucket] = sparkline[bucket].saturating_add(1);
            }
        }
    }
    if let Ok(mut g) = sparkline_cache().lock() {
        g.insert(
            cache_key.to_string(),
            (commit_sha.to_string(), today_bucket, sparkline.clone()),
        );
    }
    sparkline
}

type GitStatsCacheEntry = (std::time::Instant, GitRepoMeta);
fn git_stats_cache() -> &'static Mutex<HashMap<String, GitStatsCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, GitStatsCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn git_repo_stats(repo_path: &Path) -> GitRepoMeta {
    let cache_lookup_key = repo_path.to_string_lossy().into_owned();
    if let Some((_, cached)) = git_stats_cache()
        .lock()
        .ok()
        .and_then(|g| g.get(&cache_lookup_key).cloned())
        .filter(|(ts, _)| ts.elapsed() < std::time::Duration::from_secs(4))
    {
        return cached;
    }
    let meta = compute_git_repo_stats_uncached(repo_path);
    if let Ok(mut g) = git_stats_cache().lock() {
        g.insert(cache_lookup_key, (std::time::Instant::now(), meta.clone()));
    }
    meta
}

fn compute_git_repo_stats_uncached(repo_path: &Path) -> GitRepoMeta {
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
            main_ci_status: String::new(),
        };
    };

    let branch = repo.head_name().ok().flatten().map_or_else(
        || {
            repo.head_id()
                .ok()
                .map(|id| id.to_hex_with_len(7).to_string())
                .unwrap_or_default()
        },
        |n| n.shorten().to_string(),
    );

    let mut commit_sha = String::new();
    let mut commit_msg = String::new();
    let mut commit_ts: i64 = 0;
    let mut sparkline = vec![0u32; 30];
    let mut recent_prs = Vec::new();
    let mut main_ci_status = String::new();

    if let Ok(head_commit) = repo.head_commit() {
        commit_sha = head_commit.id().to_hex_with_len(7).to_string();
        commit_msg = head_commit
            .message()
            .ok()
            .map(|m| m.summary().to_string())
            .unwrap_or_default();
        commit_ts = head_commit.time().map_or(0, |t| t.seconds);

        let sys_now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(commit_ts, |d| d.as_secs().cast_signed());
        let now_sec = sys_now.max(commit_ts);
        let cache_key = repo_path.to_string_lossy().into_owned();

        sparkline =
            compute_repo_sparkline(&repo, head_commit.id(), &cache_key, &commit_sha, now_sec);

        // 2. Merged PRs (Merge, Squash, AND Rebase/reapply) via non-blocking gh CLI worker cached by HEAD SHA
        if let Some((cached_sha, cached_prs, cached_ci)) = pr_cache()
            .lock()
            .ok()
            .and_then(|g| g.get(&cache_key).cloned())
        {
            recent_prs = cached_prs;
            main_ci_status = cached_ci;
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
        .map_or(0, |iter| iter.filter_map(Result::ok).count());

    let repo_url = std::fs::read_to_string(repo_path.join(".git/config"))
        .ok()
        .and_then(|cfg| {
            cfg.lines()
                .find(|l| l.trim_start().starts_with("url ="))
                .and_then(|l| l.split_once('='))
                .map(|(_, u)| remote_to_github_url(u))
        })
        .unwrap_or_default();

    let repo_label = repo_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let accent = if dirty > 0 {
        (255, 159, 10)
    } else {
        (10, 132, 255)
    };
    let sparkline_png_path =
        crate::chart_png::render_velocity_chart_png(repo_label, &sparkline, accent)
            .unwrap_or_default();
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
        main_ci_status,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    NeedsAttention,
    ZipReady,
    TurnComplete,
    DropInboxZip,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NotificationPayload {
    pub repo_name: String,
    pub browser: String,
    pub window_id: i64,
    pub tab_id: i64,
    pub url: String,
    pub kind: AlertKind,
    pub raw_title: String,
}

pub fn init_system_locale() {
    static LOCALE: OnceLock<&'static str> = OnceLock::new();
    let chosen = *LOCALE.get_or_init(|| {
        let raw = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
        let lower = raw.replace('_', "-").to_lowercase();
        if lower.starts_with("zh") { "zh-CN" }
        else if lower.starts_with("fr") { "fr" }
        else if lower.starts_with("de") { "de" }
        else if lower.starts_with("es") { "es" }
        else if lower.starts_with("ja") { "ja" }
        else if lower.starts_with("ko") { "ko" }
        else { "en" }
    });
    rust_i18n::set_locale(chosen);
}

fn detect_platform_label(url: &str) -> Option<&'static str> {
    let u = url.to_lowercase();
    if u.contains("z.ai") { Some("Z.ai") }
    else if u.contains("qwen.ai") { Some("Qwen") }
    else if u.contains(concat!("gemini", ".google.com")) { Some("Gemini") }
    else if u.contains("deepseek.com") { Some("DeepSeek") }
    else if u.contains("://google.com") || u.contains("google.fr/search") || u.contains("udm=50") { Some("Google AI") }
    else { None }
}

fn format_browser_label(browser: &str) -> String {
    let b = browser.trim();
    if b.is_empty() { return "Browser".to_string(); }
    let mut chars = b.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Browser".to_string(),
    }
}

fn clean_notification_body(raw_title: &str, kind: AlertKind) -> String {
    let mut s = raw_title.trim().to_string();
    for suffix in [" - Google Search", " - Recherche Google", " - Z.ai", " - Qwen", " - Gemini", " - DeepSeek", " | Z.ai", " | Qwen"] {
        if let Some(stripped) = s.strip_suffix(suffix) { s = stripped.trim().to_string(); }
    }
    if s.is_empty() || matches!(s.to_lowercase().as_str(), "z.ai" | "qwen" | "gemini" | "deepseek" | "google" | "new chat") {
        return match kind {
            AlertKind::NeedsAttention => rust_i18n::t!("notif.body.fallback_attention").to_string(),
            AlertKind::ZipReady | AlertKind::DropInboxZip => rust_i18n::t!("notif.body.fallback_completed").to_string(),
            AlertKind::TurnComplete => rust_i18n::t!("notif.body.fallback_idle").to_string(),
        };
    }
    if s.chars().count() > 110 {
        let cut: String = s.chars().take(109).collect();
        format!("{}…", cut.trim_end())
    } else { s }
}

pub fn render_notification_copy(payload: &NotificationPayload) -> (String, String, String) {
    init_system_locale();
    let title = if payload.repo_name.trim().is_empty() { "ContextBraid".to_string() } else { payload.repo_name.trim().to_string() };
    let status_text = match payload.kind {
        AlertKind::NeedsAttention => rust_i18n::t!("notif.status.needs_attention").to_string(),
        AlertKind::ZipReady => rust_i18n::t!("notif.status.zip_ready").to_string(),
        AlertKind::TurnComplete => rust_i18n::t!("notif.status.turn_complete").to_string(),
        AlertKind::DropInboxZip => rust_i18n::t!("notif.status.drop_inbox_zip").to_string(),
    };
    let subtitle = if payload.kind == AlertKind::DropInboxZip {
        rust_i18n::t!("notif.subtitle.drop_inbox", status = status_text).to_string()
    } else {
        let browser_label = format_browser_label(&payload.browser);
        if let Some(platform) = detect_platform_label(&payload.url) {
            rust_i18n::t!("notif.subtitle.agent_with_platform", platform = platform, browser = browser_label, status = status_text).to_string()
        } else {
            rust_i18n::t!("notif.subtitle.agent_browser_only", browser = browser_label, status = status_text).to_string()
        }
    };
    (title, subtitle, clean_notification_body(&payload.raw_title, payload.kind))
}

#[cfg(test)]
static TEST_NOTIFICATIONS: std::sync::OnceLock<std::sync::Mutex<Vec<NotificationPayload>>> = std::sync::OnceLock::new();

#[cfg(test)]
pub fn take_test_notifications() -> Vec<NotificationPayload> {
    let lock = TEST_NOTIFICATIONS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    lock.lock().map(|mut guard| std::mem::take(&mut *guard)).unwrap_or_default()
}

const NOTIFY_APP_SOCK: &str = "/tmp/braid-notify-app.sock";

pub fn notify_agent_alert(payload: &NotificationPayload) {
    #[cfg(test)]
    {
        let lock = TEST_NOTIFICATIONS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
        if let Ok(mut guard) = lock.lock() { guard.push(payload.clone()); }
    }
    #[cfg(not(test))]
    {
        let sound_file = match payload.kind {
            AlertKind::ZipReady | AlertKind::DropInboxZip => "/System/Library/Sounds/Glass.aiff",
            AlertKind::NeedsAttention | AlertKind::TurnComplete => "/System/Library/Sounds/Funk.aiff",
        };
        if Path::new(sound_file).exists() { let _ = std::process::Command::new("afplay").arg(sound_file).spawn(); }
        let Ok(json_line) = serde_json::to_string(payload) else { return; };
        if send_to_notify_helper(&json_line) { return; }
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let app_bundle = home.join("Library/Application Support/ContextBraid/ContextBraid.app");
        if app_bundle.exists() {
            let _ = std::process::Command::new("open").args(["-g", "-a", &app_bundle.to_string_lossy(), "--args", "notify-agent", &json_line]).spawn();
        }
    }
}

#[cfg(not(test))]
fn send_to_notify_helper(json_line: &str) -> bool {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    if let Ok(mut stream) = UnixStream::connect(NOTIFY_APP_SOCK)
        && stream.write_all(json_line.as_bytes()).is_ok()
        && stream.write_all(b"
").is_ok()
        && stream.flush().is_ok()
    { return true; }
    false
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
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
            raw_list.cast_mut().cast::<CFArray>(),
        ))
    };

    let k_layer = CFString::from_static_str("kCGWindowLayer");
    let k_owner = CFString::from_static_str("kCGWindowOwnerName");
    let k_name = CFString::from_static_str("kCGWindowName");
    let k_pid = CFString::from_static_str("kCGWindowOwnerPID");

    let count = list.len();
    for i in 0..count {
        let ptr = unsafe { list.value_at_index(i.cast_signed()) };
        if ptr.is_null() {
            continue;
        }
        let dict = unsafe { &*(ptr.cast::<CFDictionary>()) };

        let layer_ptr = unsafe {
            dict.value(std::ptr::from_ref::<CFString>(k_layer.as_ref()).cast::<c_void>())
        };
        let mut layer: i64 = -1;
        if !layer_ptr.is_null() {
            let num = unsafe { &*(layer_ptr.cast::<CFNumber>()) };
            unsafe { num.value(CFNumberType::SInt64Type, (&raw mut layer).cast::<c_void>()) };
        }
        if layer != 0 {
            continue;
        }

        let owner_ptr = unsafe {
            dict.value(std::ptr::from_ref::<CFString>(k_owner.as_ref()).cast::<c_void>())
        };
        let owner = if owner_ptr.is_null() {
            String::new()
        } else {
            unsafe { &*(owner_ptr.cast::<CFString>()) }.to_string()
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
        let name_ptr =
            unsafe { dict.value(std::ptr::from_ref::<CFString>(k_name.as_ref()).cast::<c_void>()) };
        if !name_ptr.is_null() {
            win_name = unsafe { &*(name_ptr.cast::<CFString>()) }.to_string();
        }

        if win_name.is_empty() {
            let pid_ptr = unsafe {
                dict.value(std::ptr::from_ref::<CFString>(k_pid.as_ref()).cast::<c_void>())
            };
            let mut pid: i32 = 0;
            if !pid_ptr.is_null() {
                let num = unsafe { &*(pid_ptr.cast::<CFNumber>()) };
                unsafe { num.value(CFNumberType::SInt32Type, (&raw mut pid).cast::<c_void>()) };
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
    let err1 =
        unsafe { AXUIElementCopyAttributeValue(app_el, attr_focused.as_ref(), &raw mut win_el) };
    unsafe { CFRelease(app_el) };
    if err1 != 0 || win_el.is_null() {
        return None;
    }

    let attr_title = CFString::from_static_str("AXTitle");
    let mut title_cf: *const c_void = std::ptr::null();
    let err2 =
        unsafe { AXUIElementCopyAttributeValue(win_el, attr_title.as_ref(), &raw mut title_cf) };
    unsafe { CFRelease(win_el) };
    if err2 != 0 || title_cf.is_null() {
        return None;
    }

    let title_str: CFRetained<CFString> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
            title_cf.cast_mut().cast::<CFString>(),
        ))
    };
    let s = title_str.to_string();
    if s.is_empty() { None } else { Some(s) }
}

pub fn focus_specific_editor_window(favorite: &str, repo_name: &str) {
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

    focus_editor_app(favorite);
    let spec = resolve_editor_spec(favorite);

    let pid = find_running_app_pid(spec.bundle_id).unwrap_or(0);
    if pid <= 0 {
        return;
    }

    let app_el = unsafe { AXUIElementCreateApplication(pid) };
    if app_el.is_null() {
        return;
    }

    let attr_windows = CFString::from_static_str("AXWindows");
    let mut wins_cf: *const c_void = std::ptr::null();
    let err =
        unsafe { AXUIElementCopyAttributeValue(app_el, attr_windows.as_ref(), &raw mut wins_cf) };
    unsafe { CFRelease(app_el) };
    if err != 0 || wins_cf.is_null() {
        return;
    }

    let wins: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
            wins_cf.cast_mut().cast::<CFArray>(),
        ))
    };
    let attr_title = CFString::from_static_str("AXTitle");
    let act_raise = CFString::from_static_str("AXRaise");
    let target_low = repo_name.to_lowercase();

    for i in 0..wins.len() {
        let w_ptr = unsafe { wins.value_at_index(i.cast_signed()) };
        if w_ptr.is_null() {
            continue;
        }
        let mut title_cf: *const c_void = std::ptr::null();
        let t_err =
            unsafe { AXUIElementCopyAttributeValue(w_ptr, attr_title.as_ref(), &raw mut title_cf) };
        if t_err == 0 && !title_cf.is_null() {
            let title_str: CFRetained<CFString> = unsafe {
                CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
                    title_cf.cast_mut().cast::<CFString>(),
                ))
            };
            if title_str.to_string().to_lowercase().contains(&target_low) {
                unsafe { AXUIElementPerformAction(w_ptr, act_raise.as_ref()) };
                break;
            }
        }
    }
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
    let err =
        unsafe { AXUIElementCopyAttributeValue(app_el, attr_windows.as_ref(), &raw mut wins_cf) };
    unsafe { CFRelease(app_el) };
    if err != 0 || wins_cf.is_null() {
        return out;
    }

    let wins: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
            wins_cf.cast_mut().cast::<CFArray>(),
        ))
    };
    let attr_title = CFString::from_static_str("AXTitle");
    for i in 0..wins.len() {
        let w_ptr = unsafe { wins.value_at_index(i.cast_signed()) };
        if w_ptr.is_null() {
            continue;
        }
        let mut title_cf: *const c_void = std::ptr::null();
        let t_err =
            unsafe { AXUIElementCopyAttributeValue(w_ptr, attr_title.as_ref(), &raw mut title_cf) };
        if t_err == 0 && !title_cf.is_null() {
            let title_str: CFRetained<CFString> = unsafe {
                CFRetained::from_raw(std::ptr::NonNull::new_unchecked(
                    title_cf.cast_mut().cast::<CFString>(),
                ))
            };
            let s = title_str.to_string().trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
        }
    }
    out
}



fn send_ipc_to_daemon(msg: &braid_core::ipc::IpcMessage) {
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    static SOCK_PATH: OnceLock<String> = OnceLock::new();
    let sock_path = SOCK_PATH.get_or_init(|| {
        braid_core::BraidConfig::load_or_default()
            .map_or_else(|_| "/tmp/braid.sock".to_string(), |c| c.daemon.socket_path)
    });
    if let Ok(mut stream) = UnixStream::connect(sock_path)
        && let Ok(line) = serde_json::to_string(msg)
    {
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.write_all(b"\n");
        let _ = stream.flush();
    }
}

fn handle_notification_click(action_id: &str, browser: &str, window_id: i64, tab_id: i64, repo_name: &str) {
    if action_id == "cb-mute" && !repo_name.is_empty() {
        send_ipc_to_daemon(&braid_core::ipc::IpcMessage::ToggleRepoMute {
            repo_name: repo_name.to_string(),
            duration_ms: 30 * 60 * 1000,
        });
        return;
    }
    if tab_id > 0 && !browser.is_empty() {
        activate_browser_app(browser);
        send_ipc_to_daemon(&braid_core::ipc::IpcMessage::FocusTarget {
            browser: browser.to_string(), window_id, tab_id,
        });
    } else if let Some(home) = dirs::home_dir() {
        let _ = std::process::Command::new("/usr/bin/open").arg(home.join("Downloads")).spawn();
    }
}

fn request_notification_auth(
    center: &objc2_user_notifications::UNUserNotificationCenter,
    run_loop: &objc2_foundation::NSRunLoop,
) -> bool {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSDate, NSError};
    use objc2_user_notifications::UNAuthorizationOptions;
    use std::cell::Cell;
    use std::rc::Rc;

    let done = Rc::new(Cell::new(false));
    let granted_flag = Rc::new(Cell::new(false));
    let done_c = Rc::clone(&done);
    let granted_c = Rc::clone(&granted_flag);

    let auth_block = RcBlock::new(move |granted: Bool, _err: *mut NSError| {
        granted_c.set(granted.as_bool());
        done_c.set(true);
    });

    let opts = UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge;
    center.requestAuthorizationWithOptions_completionHandler(opts, &auth_block);

    for _ in 0..30 {
        if done.get() { break; }
        let next = NSDate::dateWithTimeIntervalSinceNow(0.05);
        run_loop.runUntilDate(&next);
    }
    granted_flag.get()
}

#[expect(clippy::too_many_lines, reason = "AppKit main event loop setup")]
pub fn run_notification_agent_app(initial_payload_json: Option<&str>) {
    use block2::Block;
    use objc2::runtime::ProtocolObject;
    use objc2::{AnyThread, MainThreadMarker, define_class, msg_send};
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEventMask};
    use objc2_foundation::{
        NSDate, NSDefaultRunLoopMode, NSDictionary, NSObject, NSObjectProtocol, NSRunLoop, NSSet, NSString,
    };
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationAction, UNNotificationActionOptions,
        UNNotificationCategory, UNNotificationCategoryOptions, UNNotificationDefaultActionIdentifier,
        UNNotificationRequest, UNNotificationResponse, UNNotificationSound,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::io::{BufRead, BufReader};
    use std::os::unix::io::AsRawFd;
    use std::os::unix::net::UnixListener;

    init_system_locale();
    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "BraidNotificationDelegate"]
        struct BraidNotificationDelegate;
        unsafe impl NSObjectProtocol for BraidNotificationDelegate {}
        unsafe impl UNUserNotificationCenterDelegate for BraidNotificationDelegate {
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive_response(
                &self,
                _center: &UNUserNotificationCenter,
                response: &UNNotificationResponse,
                completion_handler: &Block<dyn Fn()>,
            ) {
                let action_id = response.actionIdentifier().to_string();
                let content = response.notification().request().content();
                let user_info = content.userInfo();
                let get_str = |k: &str| -> String {
                    let key = NSString::from_str(k);
                    match user_info.objectForKey(&key) {
                        Some(obj) => match obj.downcast::<NSString>() {
                            Ok(ns_s) => ns_s.to_string(),
                            Err(_) => String::new(),
                        },
                        None => String::new(),
                    }
                };
                let browser = get_str("browser");
                let window_id: i64 = str::parse(&get_str("window_id")).unwrap_or(0);
                let tab_id: i64 = str::parse(&get_str("tab_id")).unwrap_or(0);
                let repo_name = get_str("repo_name");
                let default_act = unsafe { UNNotificationDefaultActionIdentifier.to_string() };
                if action_id == default_act || action_id == "cb-focus" || action_id == "cb-mute" {
                    handle_notification_click(&action_id, &browser, window_id, tab_id, &repo_name);
                }
                completion_handler.call(());
            }
        }
    );

    let Some(mtm) = MainThreadMarker::new() else { return; };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();

    let center = UNUserNotificationCenter::currentNotificationCenter();
    let delegate: objc2::rc::Retained<BraidNotificationDelegate> =
        unsafe { msg_send![BraidNotificationDelegate::alloc(), init] };
    let delegate_proto = ProtocolObject::from_ref(&*delegate);
    center.setDelegate(Some(delegate_proto));

    let focus_title = NSString::from_str(&rust_i18n::t!("notif.action.focus_tab"));
    let mute_title = NSString::from_str(&rust_i18n::t!("notif.action.mute_repo"));
    let focus_action = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str("cb-focus"), &focus_title, UNNotificationActionOptions::Foreground,
    );
    let mute_action = UNNotificationAction::actionWithIdentifier_title_options(
        &NSString::from_str("cb-mute"), &mute_title, UNNotificationActionOptions::empty(),
    );
    let actions = objc2_foundation::NSArray::from_slice(&[&*focus_action, &*mute_action]);
    let category = UNNotificationCategory::categoryWithIdentifier_actions_intentIdentifiers_options(
        &NSString::from_str("cb-agent-alert"), &actions, &objc2_foundation::NSArray::new(),
        UNNotificationCategoryOptions::empty(),
    );
    center.setNotificationCategories(&NSSet::from_slice(&[&*category]));

    let authorized = request_notification_auth(&center, &NSRunLoop::currentRunLoop());

    let post_payload = |raw_json: &str| {
        let Ok(payload) = serde_json::from_str::<NotificationPayload>(raw_json) else { return; };
        let (title, subtitle, body) = render_notification_copy(&payload);
        if authorized {
            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(&title));
            content.setSound(Some(&UNNotificationSound::defaultSound()));
            content.setSubtitle(&NSString::from_str(&subtitle));
            content.setBody(&NSString::from_str(&body));
            content.setCategoryIdentifier(&NSString::from_str("cb-agent-alert"));
            content.setThreadIdentifier(&NSString::from_str(&format!("cb-repo-{}", payload.repo_name)));
            let k_browser = NSString::from_str("browser");
            let k_win = NSString::from_str("window_id");
            let k_tab = NSString::from_str("tab_id");
            let k_repo = NSString::from_str("repo_name");
            let v_browser = NSString::from_str(&payload.browser);
            let v_win = NSString::from_str(&payload.window_id.to_string());
            let v_tab = NSString::from_str(&payload.tab_id.to_string());
            let v_repo = NSString::from_str(&payload.repo_name);
            let user_info = NSDictionary::from_slices(
                &[&*k_browser, &*k_win, &*k_tab, &*k_repo],
                &[&*v_browser, &*v_win, &*v_tab, &*v_repo],
            );
            let raw_dict: &NSDictionary = unsafe { &*(objc2::rc::Retained::as_ptr(&user_info).cast()) };
            unsafe { content.setUserInfo(raw_dict) };
            let req_id = format!(
                "cb-{}-{}-{}", payload.browser, payload.tab_id,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
            );
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(&req_id), &content, None,
            );
            center.addNotificationRequest_withCompletionHandler(&request, None);
        } else {
            let full = format!("ContextBraid • {title}");
            let script = format!("display notification {body:?} with title {full:?} subtitle {subtitle:?}");
            let _ = std::process::Command::new("/usr/bin/osascript").args(["-e", &script]).spawn();
        }
    };

    if let Some(init_json) = initial_payload_json { post_payload(init_json); }

    let lock_file = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open("/tmp/braid-notify-app.lock");
    let got_lock = lock_file.as_ref().is_ok_and(|f| unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0);

    let pump_appkit = |secs: f64| {

let deadline = NSDate::dateWithTimeIntervalSinceNow(secs);
loop {
    let ev = unsafe {
        app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&deadline), NSDefaultRunLoopMode, true)
    };
    match ev { Some(event) => app.sendEvent(&event), None => break }
}
app.updateWindows();
};
if !got_lock { pump_appkit(0.8); return; }
let _ = std::fs::remove_file(NOTIFY_APP_SOCK);
let Ok(listener) = UnixListener::bind(NOTIFY_APP_SOCK) else { return; };
let _ = listener.set_nonblocking(true);
loop {
    while let Ok((stream, _)) = listener.accept() {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let trimmed = line.trim();
            if !trimmed.is_empty() { post_payload(trimmed); }
        }
    }
    pump_appkit(0.15);
}
}
#[derive(Debug, Clone)]
pub struct ScreenWindowRect {
    pub owner_low: String,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGRectMakeWithDictionaryRepresentation(dict: *const c_void, rect: *mut CGRectRaw) -> bool;
}
#[cfg_attr(test, expect(dead_code, reason = "Used in non-test builds"))]
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CGRectRaw { x: f64, y: f64, width: f64, height: f64 }
#[cfg_attr(test, expect(dead_code, reason = "Used in non-test builds"))]
pub fn capture_onscreen_window_stack() -> Vec<ScreenWindowRect> {
    use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFNumberType, CFRetained, CFString};
    let mut out = Vec::new();
    let raw_list = unsafe { CGWindowListCopyWindowInfo(17, 0) };
    if raw_list.is_null() { return out; }
    let list: CFRetained<CFArray> = unsafe {
        CFRetained::from_raw(std::ptr::NonNull::new_unchecked(raw_list.cast_mut().cast::<CFArray>()))
    };
    let k_layer = CFString::from_static_str("kCGWindowLayer");
    let k_owner = CFString::from_static_str("kCGWindowOwnerName");
    let k_bounds = CFString::from_static_str("kCGWindowBounds");
    for i in 0..list.len() {
        let ptr = unsafe { list.value_at_index(i.cast_signed()) };
        if ptr.is_null() { continue; }
        let dict: &CFDictionary = unsafe { &*ptr.cast::<CFDictionary>() };
        let layer_ptr = unsafe { dict.value(std::ptr::from_ref::<CFString>(k_layer.as_ref()).cast::<c_void>()) };
        let mut layer: i64 = -1;
        if !layer_ptr.is_null() {
            let num: &CFNumber = unsafe { &*layer_ptr.cast::<CFNumber>() };
            unsafe { num.value(CFNumberType::SInt64Type, (&raw mut layer).cast::<c_void>()) };
        }
        if layer != 0 { continue; }
        let owner_ptr = unsafe { dict.value(std::ptr::from_ref::<CFString>(k_owner.as_ref()).cast::<c_void>()) };
        let owner_low = if owner_ptr.is_null() { String::new() } else {
            let cf_s: &CFString = unsafe { &*owner_ptr.cast::<CFString>() };
            cf_s.to_string().to_lowercase()
        };
        if owner_low.is_empty() || matches!(owner_low.as_str(), "window server" | "dock" | "control center" | "notification center") {
            continue;
        }
        let bounds_ptr = unsafe { dict.value(std::ptr::from_ref::<CFString>(k_bounds.as_ref()).cast::<c_void>()) };
        if bounds_ptr.is_null() { continue; }
        let mut rect = CGRectRaw::default();
        if unsafe { CGRectMakeWithDictionaryRepresentation(bounds_ptr, &raw mut rect) } && rect.width > 40.0 && rect.height > 40.0 {
            out.push(ScreenWindowRect { owner_low, left: rect.x, top: rect.y, width: rect.width, height: rect.height });
        }
    }
    out
}
pub fn is_window_unobstructed_on_screen(
  stack: &[ScreenWindowRect],
  browser: &str,
  bounds: Option<braid_core::WindowBounds>,
) -> bool {
    if stack.is_empty() { return true; }
    let browser_low = browser.to_lowercase();
    let Some(b) = bounds else {
        return stack.first().is_some_and(|top| top.owner_low.contains(&browser_low));
    };
    if b.width <= 40.0 || b.height <= 40.0 { return false; }
    let Some(win_idx) = stack.iter().position(|w| {
          w.owner_low.contains(&browser_low)
          && (w.left - b.left).abs() <= 24.0 && (w.top - b.top).abs() <= 24.0
          && (w.width - b.width).abs() <= 32.0 && (w.height - b.height).abs() <= 32.0
    }) else { return false; };
    let occluders = &stack[..win_idx];
    if occluders.is_empty() { return true; }
    let mut visible_samples = 0u8;
    for gx in 0..4 {
        let px = b.left + b.width * (f64::from(gx) + 0.5) / 4.0;
        for gy in 0..4 {
            let py = b.top + b.height * (f64::from(gy) + 0.5) / 4.0;
            let occluded = occluders.iter().any(|occ| {
                  px >= occ.left && px <= occ.left + occ.width && py >= occ.top && py <= occ.top + occ.height
            });
            if !occluded { visible_samples += 1; }
        }
    }
    visible_samples >= 4
}
#[cfg(test)]
mod notif_i18n_tests {
    use super::*;
    #[test]
    fn test_notification_i18n_renders_real_copy() {
        let p = NotificationPayload {
            repo_name: "glyim".to_string(), browser: "brave".to_string(),
            window_id: 1, tab_id: 2, url: "https://z.ai".to_string(),
            kind: AlertKind::TurnComplete, raw_title: "Glyim".to_string(),
        };
        let (title, subtitle, body) = render_notification_copy(&p);
        assert_eq!(title, "glyim");
        assert!(!subtitle.contains("notif."), "raw key leaked: {subtitle}");
        assert!(subtitle.contains("Z.ai") && subtitle.contains("Brave"), "unexpected subtitle: {subtitle}");
        assert_eq!(body, "Glyim");
    }
    #[test]
    fn test_platform_detection_and_ssh_remote_urls() {
        let gemini_url = ["https", ":", "//", "gemini", ".google.com", "/app/abc"].join("");
        let p_gem = NotificationPayload {
            repo_name: "demo".to_string(),
            browser: "brave".to_string(),
            window_id: 1,
            tab_id: 2,
            url: gemini_url,
            kind: AlertKind::TurnComplete,
            raw_title: "Chat".to_string(),
        };
        let (_, sub_gem, _) = render_notification_copy(&p_gem);
        assert!(sub_gem.contains("Gemini"), "Expected Gemini in subtitle, got: {sub_gem}");

        let gai_url = ["https", ":", "//", "www", ".google.com", "/search?q=rust&udm=50"].join("");
        let p_gai = NotificationPayload { url: gai_url, ..p_gem };
        let (_, sub_gai, _) = render_notification_copy(&p_gai);
        assert!(sub_gai.contains("Google AI"), "Expected Google AI in subtitle, got: {sub_gai}");

        let ssh_in = ["ssh", ":", "//", "git@", "github.com", "/elcoosp/context-braid.git"].join("");
        let gh_out = ["https", ":", "//", "github.com", "/elcoosp/context-braid"].join("");
        assert_eq!(remote_to_github_url(&ssh_in), gh_out);
    }

    #[test]
    fn test_2d_zorder_occlusion_split_vs_covered() {
        let stack = vec![
          ScreenWindowRect { owner_low: "brave browser".into(), left: 0.0, top: 25.0, width: 720.0, height: 875.0 },
          ScreenWindowRect { owner_low: "brave browser".into(), left: 720.0, top: 25.0, width: 720.0, height: 875.0 },
        ];
        assert!(is_window_unobstructed_on_screen(&stack, "brave", Some(braid_core::WindowBounds { left: 720.0, top: 25.0, width: 720.0, height: 875.0 })));
        let mut covered_stack = vec![ScreenWindowRect { owner_low: "zed".into(), left: 700.0, top: 0.0, width: 800.0, height: 900.0 }];
        covered_stack.extend(stack);
        assert!(!is_window_unobstructed_on_screen(&covered_stack, "brave", Some(braid_core::WindowBounds { left: 720.0, top: 25.0, width: 720.0, height: 875.0 })));
    }
}
