use anyhow::Result;
use braid_core::BraidConfig;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn read_frame<R: Read>(r: &mut R) -> Result<Option<String>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)?;
    Ok(Some(String::from_utf8(buf)?))
}

pub fn write_frame<W: Write>(w: &mut W, json: &str) -> Result<()> {
    let b = json.as_bytes();
    w.write_all(&(b.len() as u32).to_le_bytes())?;
    w.write_all(b)?;
    w.flush()?;
    Ok(())
}

fn connect_with_retry(sock_path: &str) -> Result<UnixStream> {
    let mut last_err = None;
    for _ in 0..30 {
        match UnixStream::connect(sock_path) {
            Ok(s) => return Ok(s),
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Err(anyhow::anyhow!(
        "Socket inaccessible ({}): {:?}",
        sock_path,
        last_err
    ))
}

pub fn run_host() -> Result<()> {
    let cfg = BraidConfig::load_or_default()?;
    let sock_path = cfg.daemon.socket_path.clone();
    let stdout_lock = Arc::new(Mutex::new(io::stdout()));

    // Dedicated push subscription stream for instant (< 1ms) focus_tab commands
    let push_sock_path = sock_path.clone();
    let push_stdout = Arc::clone(&stdout_lock);
    thread::spawn(move || {
        loop {
            if let Ok(mut sub) = connect_with_retry(&push_sock_path) {
                let _ = writeln!(sub, r#"{{"type":"subscribe_host","browser":"all"}}"#);
                let _ = sub.flush();
                let mut reader = BufReader::new(sub);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line) {
                    if n == 0 {
                        break;
                    }
                    let trimmed = line.trim();
                    if !trimmed.is_empty()
                        && let Ok(mut out) = push_stdout.lock()
                    {
                        let _ = write_frame(&mut *out, trimmed);
                    }
                    line.clear();
                }
            }
            thread::sleep(Duration::from_millis(200));
        }
    });

    let mut sock = connect_with_retry(&sock_path)?;
    let mut reader = BufReader::new(sock.try_clone()?);
    let stdin = io::stdin();
    let mut in_l = stdin.lock();

    while let Some(msg) = read_frame(&mut in_l)? {
        let mut resp = r#"{"type":"ack"}"#.to_string();
        for _ in 0..3 {
            if writeln!(sock, "{}", msg.trim()).is_ok() && sock.flush().is_ok() {
                let mut line = String::new();
                if let Ok(n) = reader.read_line(&mut line)
                    && n > 0
                    && !line.trim().is_empty()
                {
                    resp = line.trim().to_string();
                    break;
                }
            }
            sock = connect_with_retry(&sock_path)?;
            reader = BufReader::new(sock.try_clone()?);
        }
        if let Ok(mut out) = stdout_lock.lock() {
            write_frame(&mut *out, &resp)?;
        }
    }
    Ok(())
}

pub fn run_host_entry() {
    if let Err(e) = run_host() {
        eprintln!("[braid-host] fatal: {:?}", e);
        std::process::exit(1);
    }
}
