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
    pub proc_names: &'static [&'static str],
}

pub fn resolve_editor_spec(favorite: &str) -> EditorSpec {
    match favorite.trim().to_lowercase().as_str() {
        "cursor" => EditorSpec {
            id: "cursor",
            label: "Cursor",
            bundle_id: "com.todesktop.230313mzl4w4u92",
            cli_app: "Cursor",
            proc_names: &["Cursor Helper", "Cursor"],
        },
        "vscode" | "code" | "visual studio code" => EditorSpec {
            id: "vscode",
            label: "VS Code",
            bundle_id: "com.microsoft.VSCode",
            cli_app: "Visual Studio Code",
            proc_names: &["Code Helper", "Electron"],
        },
        "windsurf" => EditorSpec {
            id: "windsurf",
            label: "Windsurf",
            bundle_id: "com.exafunction.windsurf",
            cli_app: "Windsurf",
            proc_names: &["Windsurf Helper", "Windsurf"],
        },
        "neovim" | "nvim" => EditorSpec {
            id: "neovim",
            label: "Neovim",
            bundle_id: "com.mitchellh.ghostty",
            cli_app: "Ghostty",
            proc_names: &["nvim"],
        },
        _ => EditorSpec {
            id: "zed",
            label: "Zed",
            bundle_id: "dev.zed.Zed",
            cli_app: "Zed",
            proc_names: &["zed", "Zed"],
        },
    }
}

pub fn list_editor_windows(repos_root: &Path, favorite: &str) -> Vec<PathBuf> {
    let spec = resolve_editor_spec(favorite);
    let mut pids = Vec::new();
    for proc_name in spec.proc_names {
        if let Ok(o) = Command::new("pgrep").args(["-x", proc_name]).output() {
            if o.status.success() {
                for pid in String::from_utf8_lossy(&o.stdout).split_whitespace() {
                    if !pids.contains(&pid.to_string()) {
                        pids.push(pid.to_string());
                    }
                }
            }
        }
    }
    if pids.is_empty() {
        return Vec::new();
    }

    let mut open_paths = Vec::new();
    if let Ok(out) = Command::new("lsof")
        .args(["-a", "-p", &pids.join(","), "-Fn"])
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

pub fn focus_editor_app(favorite: &str) -> Result<()> {
    let spec = resolve_editor_spec(favorite);
    unsafe {
        activate_bundle_id(spec.bundle_id);
    }
    Ok(())
}

pub fn open_in_favorite_editor(favorite: &str, repo_path: &str) -> Result<()> {
    let spec = resolve_editor_spec(favorite);
    let _ = Command::new("open").args(["-a", spec.cli_app, repo_path]).output();
    unsafe {
        activate_bundle_id(spec.bundle_id);
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

pub fn git_repo_stats(repo_path: &Path) -> (String, usize) {
    let Ok(repo) = gix::open_opts(repo_path, gix::open::Options::isolated()) else {
        return (String::new(), 0);
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

    let dirty = repo
        .status(gix::progress::Discard)
        .ok()
        .and_then(|platform| platform.into_index_worktree_iter(Vec::new()).ok())
        .map(|iter| iter.filter_map(Result::ok).count())
        .unwrap_or(0);

    (branch, dirty)
}
