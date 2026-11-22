use anyhow::{Context, Result, ensure};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TempDirGuard {
    dir: PathBuf,
    sock: PathBuf,
}
impl TempDirGuard {
    fn new() -> Result<Self> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("contextbraid-e2e-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir)?;
        let sock = PathBuf::from(format!("/tmp/cb-e2e-{pid}.sock"));
        let _ = std::fs::remove_file(&sock);
        Ok(Self { dir, sock })
    }
    fn path(&self) -> &Path {
        &self.dir
    }
    fn sock_path(&self) -> &Path {
        &self.sock
    }
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.sock);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn frame_native_msg(payload: &serde_json::Value) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(payload)?;
    let len = u32::try_from(bytes.len())?.to_le_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&bytes);
    Ok(out)
}

fn read_native_msg(reader: &mut impl Read) -> Result<String> {
    let mut len_buf = [0u8; 4];
    reader
        .read_exact(&mut len_buf)
        .context("lecture trame Native Messaging")?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut bytes = vec![0u8; len];
    reader.read_exact(&mut bytes)?;
    Ok(String::from_utf8(bytes)?)
}

fn init_temp_git_repo(repo_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(repo_dir)?;
    ensure!(
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(repo_dir)
            .status()?
            .success(),
        "git init failed"
    );
    ensure!(
        Command::new("git")
            .args(["config", "user.email", "ci@contextbraid.dev"])
            .current_dir(repo_dir)
            .status()?
            .success(),
        "git config email failed"
    );
    ensure!(
        Command::new("git")
            .args(["config", "user.name", "CI"])
            .current_dir(repo_dir)
            .status()?
            .success(),
        "git config name failed"
    );
    std::fs::write(repo_dir.join("README.md"), "# Demo Cockpit\n")?;
    ensure!(
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir)
            .status()?
            .success(),
        "git add failed"
    );
    ensure!(
        Command::new("git")
            .args(["commit", "-m", "feat: initial commit"])
            .current_dir(repo_dir)
            .status()?
            .success(),
        "git commit failed"
    );
    Ok(())
}

fn verify_host_roundtrip(bin: &str, fake_home: &Path, repo_dir: &Path) -> Result<()> {
    let mut host = Command::new(bin)
        .arg("host")
        .env("HOME", fake_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn braid host")?;
    let msg = serde_json::json!({
        "type": "sync_tabs",
        "browser": "brave",
        "tabs": [{
            "browser": "brave",
            "window_id": 1,
            "tab_id": 42,
            "opener_tab_id": null,
            "incognito": false,
            "url": "https://chatgpt.com",
            "title": "demo-cockpit — Implement E2E pipeline",
            "context_snippet": format!("Working in {}", repo_dir.display()),
            "status": "working",
            "updated_at_ms": 1_760_000_000_000_u64,
        }]
    });
    let mut stdin = host.stdin.take().context("host stdin")?;
    stdin.write_all(&frame_native_msg(&msg)?)?;
    stdin.flush()?;

    let mut host_out = host.stdout.take().context("host stdout")?;
    let mut ack_str = String::new();
    for _ in 0..4 {
        let frame = read_native_msg(&mut host_out)?;
        if frame.contains(r#""type":"ack""#) {
            ack_str = frame;
            break;
        }
    }
    drop(stdin);
    ensure!(
        ack_str.contains(r#""type":"ack""#),
        "ACK inattendu: {ack_str}"
    );
    let _ = host.wait();
    Ok(())
}

#[test]
fn e2e_host_to_daemon_to_raycast_socket_roundtrip() -> Result<()> {
    let tmp = TempDirGuard::new()?;
    let fake_home = tmp.path().join("home");
    let repos_root = fake_home.join("Documents/Repos");
    let repo_dir = repos_root.join("demo-cockpit");
    init_temp_git_repo(&repo_dir)?;

    let sock_path = tmp.sock_path();
    let cfg_dir = fake_home.join("Library/Application Support/contextbraid");
    std::fs::create_dir_all(&cfg_dir)?;
    let toml_cfg = format!(
        "[repos]\nroot_dir = \"{}\"\n\n[daemon]\nsocket_path = \"{}\"\n\n[editor]\nfavorite = \"zed\"\n",
        repos_root.display(),
        sock_path.display()
    );
    std::fs::write(cfg_dir.join("config.toml"), toml_cfg)?;

    let bin = env!("CARGO_BIN_EXE_braid");
    let lock_path = tmp.path().join("daemon.lock");
    let mut daemon = Command::new(bin)
        .arg("daemon")
        .env("HOME", &fake_home)
        .env("CONTEXT_BRAID_LOCK_FILE", &lock_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn braid daemon")?;

    let deadline = Instant::now() + Duration::from_secs(6);
    while !sock_path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    if !sock_path.exists() {
        let _ = daemon.kill();
        let out = daemon.wait_with_output()?;
        anyhow::bail!(
            "braid.sock non créé dans les 6s (stderr={})",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let roundtrip_res = (|| -> Result<()> {
        verify_host_roundtrip(bin, &fake_home, &repo_dir)?;
        let mut client = UnixStream::connect(sock_path).context("connexion braid.sock")?;
        client.set_read_timeout(Some(Duration::from_secs(3)))?;
        client.write_all(b"{\"type\":\"get_dashboard\"}\n")?;
        client.flush()?;
        let mut line = String::new();
        BufReader::new(client).read_line(&mut line)?;
        let state: serde_json::Value = serde_json::from_str(&line).context("JSON state valide")?;
        let projects = state["projects"].as_array().context("liste projects")?;
        let proj = projects
            .iter()
            .find(|p| p["repo_name"] == "demo-cockpit")
            .context("repo demo-cockpit absent du state")?;
        let tabs = proj["tabs"].as_array().context("liste tabs")?;
        ensure!(
            tabs.iter()
                .any(|tab| tab["tab_id"] == 42 && tab["status"] == "working"),
            "tab 42 absent du state"
        );
        Ok(())
    })();

    let _ = daemon.kill();
    let _ = daemon.wait();
    roundtrip_res
}
