//! JSON control socket, replacing the `MpcApi.h` WM_COPYDATA API. It runs on its own threads;
//! requests are sent to the GTK main thread as a `Job` and answered there.
//!
//! Socket: `$XDG_RUNTIME_DIR/mpc-rs.sock`, one JSON object per line. Requests are MpcApi's commands
//! without the `CMD_` prefix, e.g. `{"cmd":"OPENFILE","path":"/x.mkv"}`, `{"cmd":"SETPOSITION","seconds":90}`,
//! `{"cmd":"WM_COMMAND","id":889}`; each gets one reply line (`{"ok":true,...}` or
//! `{"ok":false,"error":"..."}`). The player also sends events: `{"event":"STATE","state":"Playing"}`,
//! `{"event":"NOWPLAYING",...}`, `{"event":"NOTIFYSEEK",...}`, `{"event":"NOTIFYENDOFSTREAM"}`.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

/// A request for the main thread, and where to send its answer.
pub struct Job {
    pub request: Value,
    pub reply: mpsc::Sender<Value>,
}

/// Ask the main thread and wait (it answers on its next tick).
fn ask(jobs: &mpsc::Sender<Job>, request: Value) -> Option<Value> {
    let (tx, rx) = mpsc::channel();
    jobs.send(Job { request, reply: tx }).ok()?;
    rx.recv_timeout(Duration::from_secs(10)).ok()
}

pub type Clients = Arc<Mutex<Vec<UnixStream>>>;

pub fn socket_path() -> PathBuf {
    gtk4::glib::user_runtime_dir().join("mpc-rs.sock")
}

/// Listen on the control socket. Returns the connected clients, for `broadcast`.
pub fn start_ipc(jobs: mpsc::Sender<Job>) -> Option<Clients> {
    let path = socket_path();
    // A socket left by a crashed run would block the bind; a live one answers, so keep that.
    if UnixStream::connect(&path).is_ok() {
        eprintln!("{}: another player is listening", path.display());
        return None;
    }
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).map_err(|e| eprintln!("{}: {e}", path.display())).ok()?;
    let clients: Clients = Arc::default();
    let c = clients.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Ok(w) = stream.try_clone() {
                c.lock().unwrap().push(w);
            }
            let jobs = jobs.clone();
            std::thread::spawn(move || {
                let mut out = match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    let reply = match serde_json::from_str::<Value>(&line) {
                        Ok(v) => ask(&jobs, v).unwrap_or_else(|| json!({"ok": false, "error": "no answer"})),
                        Err(e) => json!({"ok": false, "error": e.to_string()}),
                    };
                    if writeln!(out, "{reply}").is_err() {
                        break;
                    }
                }
            });
        }
    });
    Some(clients)
}

/// Send an event line to every connected client (dropping the ones that went away).
pub fn broadcast(clients: &Clients, event: &Value) {
    let line = format!("{event}\n");
    clients.lock().unwrap().retain_mut(|c| c.write_all(line.as_bytes()).is_ok());
}

pub fn cleanup_socket() {
    let _ = std::fs::remove_file(socket_path());
}
