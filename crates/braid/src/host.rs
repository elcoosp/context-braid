use anyhow::Result;
use braid_core::BraidConfig;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

fn log_err(msg: &str) {
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/braid-host.err.log")
    {
        let _ = writeln!(f, "[pid={}] {}", std::process::id(), msg);
    }
}

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
    Err(anyhow::anyhow!("Socket inaccessible ({}): {:?}", sock_path, last_err))
}

fn exchange_with_daemon(
    sock: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    sock_path: &str,
    payload: &str,
) -> Result<String> {
    for _ in 0..3 {
        if writeln!(sock, "{}", payload).is_ok() && sock.flush().is_ok() {
            let mut resp = String::new();
            if let Ok(n) = reader.read_line(&mut resp) {
                if n > 0 && !resp.trim().is_empty() {
                    return Ok(resp);
                }
            }
        }
        *sock = connect_with_retry(sock_path)?;
        *reader = BufReader::new(sock.try_clone()?);
    }
    Ok(r#"{"type":"ack"}"#.to_string())
}

fn trace_host(msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("/tmp/braid-host.trace.log") {
        let _ = writeln!(f, "[pid={}] {}", std::process::id(), msg);
    }
}

pub fn run_host() -> Result<()> {
    trace_host("HOST_SPAWNED by browser");
    let cfg = BraidConfig::load_or_default()?;
    let sock_path = cfg.daemon.socket_path.clone();
    let mut sock = connect_with_retry(&sock_path)?;
    let mut reader = BufReader::new(sock.try_clone()?);
    let (stdin, stdout) = (io::stdin(), io::stdout());
    let (mut in_l, mut out_l) = (stdin.lock(), stdout.lock());

    while let Some(msg) = read_frame(&mut in_l)? {
        trace_host(&format!("RX_FRAME len={}", msg.len()));
        let resp = exchange_with_daemon(&mut sock, &mut reader, &sock_path, msg.trim())?;
        write_frame(&mut out_l, resp.trim())?;
    }
    Ok(())
}

pub fn run_host_entry() {
    if let Err(e) = run_host() {
        log_err(&format!("ERREUR FATALE: {:?}", e));
        std::process::exit(1);
    }
}
