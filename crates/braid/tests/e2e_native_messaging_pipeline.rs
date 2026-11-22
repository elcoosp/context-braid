use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn frame_native_msg(json_payload: &str) -> Vec<u8> {
    let bytes = json_payload.as_bytes();
    let len = (bytes.len() as u32).to_le_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(bytes);
    out
}

#[test]
fn e2e_host_to_daemon_to_raycast_socket_roundtrip() {
    let pid = std::process::id();
    let tmp = PathBuf::from(format!("/tmp/cb-e2e-{pid}"));
    let sock_path = PathBuf::from(format!("/tmp/cb-e2e-{pid}.sock"));
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_file(&sock_path);

    let fake_home = tmp.join("home");
    let repo_dir = fake_home.join("repos").join("demo-cockpit");
    std::fs::create_dir_all(&repo_dir).unwrap();

    assert!(Command::new("git").args(["init", "-b", "main"]).current_dir(&repo_dir).status().unwrap().success());
    assert!(Command::new("git").args(["config", "user.email", "ci@contextbraid.dev"]).current_dir(&repo_dir).status().unwrap().success());
    assert!(Command::new("git").args(["config", "user.name", "CI"]).current_dir(&repo_dir).status().unwrap().success());
    std::fs::write(repo_dir.join("README.md"), "# Demo Cockpit\n").unwrap();
    assert!(Command::new("git").args(["add", "."]).current_dir(&repo_dir).status().unwrap().success());
    assert!(Command::new("git").args(["commit", "-m", "feat: initial commit"]).current_dir(&repo_dir).status().unwrap().success());

    let app_support = fake_home.join("Library").join("Application Support");
    let cfg_dir = app_support.join("contextbraid");
    std::fs::create_dir_all(&cfg_dir).unwrap();

    let lock_path = tmp.join("braid.daemon.lock");
    let scan_root = fake_home.join("repos");
    let toml_cfg = format!(
        "[repos]\nroot_dir = \"{}\"\n\n[daemon]\nsocket_path = \"{}\"\n",
        scan_root.display(),
        sock_path.display()
    );
    std::fs::write(cfg_dir.join("config.toml"), toml_cfg).unwrap();

    let bin = env!("CARGO_BIN_EXE_braid");

    // 1. Démarre un vrai processus `braid daemon` isolé via HOME + CONTEXT_BRAID_LOCK_FILE
    let mut daemon = Command::new(bin)
        .arg("daemon")
        .env("HOME", &fake_home)
        .env("CONTEXT_BRAID_LOCK_FILE", &lock_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn braid daemon");

    let deadline = Instant::now() + Duration::from_secs(6);
    while !sock_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    if !sock_path.exists() {
        let _ = daemon.kill();
        let out = daemon.wait_with_output().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
        panic!(
            "braid.sock non créé dans les 6s (stderr={})",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // 2. Envoie un message Native Messaging binaire via `braid host`
    let mut host = Command::new(bin)
        .arg("host")
        .env("HOME", &fake_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn braid host");

    let msg = serde_json::json!({
        "type": "tabs_update",
        "browser": "Brave",
        "tabs": [{
            "browser": "Brave",
            "window_id": 10,
            "tab_id": 42,
            "opener_tab_id": null,
            "incognito": false,
            "url": "https://z.ai",
            "title": "Working on demo-cockpit",
            "context_snippet": "Building feature",
            "status": "working",
            "updated_at_ms": 1760000000000u64,
            "matched_repo": "demo-cockpit",
            "zip_download_url": null
        }]
    })
    .to_string();

    {
        let mut stdin = host.stdin.take().unwrap();
        stdin.write_all(&frame_native_msg(&msg)).unwrap();
        stdin.flush().unwrap();
    }

    let mut host_out = host.stdout.take().unwrap();
    let mut len_buf = [0u8; 4];
    host_out.read_exact(&mut len_buf).expect("lecture ack Native Messaging");
    let ack_len = u32::from_le_bytes(len_buf) as usize;
    let mut ack_bytes = vec![0u8; ack_len];
    host_out.read_exact(&mut ack_bytes).unwrap();
    let ack_str = String::from_utf8(ack_bytes).unwrap();
    assert!(ack_str.contains("ok") || ack_str.contains("ack") || ack_str.contains("sync_config"), "Ack inattendu: {ack_str}");

    let _ = host.kill();
    let _ = host.wait();

    // 3. Interroge `braid.sock` comme le client Raycast (`get_state`)
    let mut client = UnixStream::connect(&sock_path).expect("connexion braid.sock");
    client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    client.write_all(b"{\"type\":\"get_state\"}\n").unwrap();
    client.flush().unwrap();

    let mut line = String::new();
    BufReader::new(client).read_line(&mut line).unwrap();
    let state: serde_json::Value = serde_json::from_str(&line).expect("JSON state valide");

    let repos = state["repos"].as_array().expect("liste repos");
    assert!(
        repos.iter().any(|r| r["name"] == "demo-cockpit"),
        "Le dépôt Git demo-cockpit doit être détecté: {state}"
    );
    let tabs = state["tabs"].as_array().expect("liste tabs");
    assert!(
        tabs.iter().any(|t| t["tab_id"] == 42 && t["matched_repo"] == "demo-cockpit"),
        "L'onglet Brave envoyé via Native Messaging doit être fusionné dans l'état: {state}"
    );

    let _ = daemon.kill();
    let _ = daemon.wait();
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir_all(&tmp);
}
