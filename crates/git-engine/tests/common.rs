//! Shared fixtures for git-engine integration tests.
//!
//! All fixtures are local: bare repos seeded with `git init --bare`
//! and local clones. No network, no GitHub credentials. Fake HTTP
//! servers (for credential / cancellation tests) listen on
//! 127.0.0.1 ephemeral ports.
//!
//! Every test binary compiles this module, but each uses only part
//! of it — dead-code warnings are disabled here on purpose.

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Run `git <args>` (optionally in `cwd`), return the output.
pub async fn run_git(args: &[&str], cwd: Option<&Path>) -> std::process::Output {
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(args);
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.output().await.expect("git fixture command failed")
}

/// Configure a throwaway identity in a work clone.
pub async fn set_identity(work: &Path) {
    run_git(&["config", "user.email", "test@reponyx.local"], Some(work)).await;
    run_git(&["config", "user.name", "RepoNyx Test"], Some(work)).await;
}

/// Write a file, add it, and commit it in `work`.
pub async fn commit_file(work: &Path, rel: &str, content: &[u8], message: &str) {
    let path = work.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
    run_git(&["add", "."], Some(work)).await;
    let out = run_git(&["commit", "-m", message], Some(work)).await;
    assert!(
        out.status.success(),
        "commit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A seeded bare repo: `main` + `feature` branches, an annotated tag
/// `v1.0.0`, a lightweight tag `v0.9`, and a `doomed` branch (for
/// prune tests).
pub struct RichFixture {
    pub bare: PathBuf,
    pub head_sha: String,
    pub feature_sha: String,
    pub annotated_tag_sha: String,
    pub annotated_tag_object: String,
    pub light_tag_sha: String,
}

pub async fn seeded_bare_repo(dir: &Path) -> RichFixture {
    let bare = dir.join("source.git");
    let out = run_git(&["init", "--bare", bare.to_str().unwrap()], None).await;
    assert!(out.status.success(), "init --bare failed");

    let work = dir.join("seed-work");
    let out = run_git(
        &["clone", bare.to_str().unwrap(), work.to_str().unwrap()],
        None,
    )
    .await;
    assert!(out.status.success(), "seed clone failed");
    set_identity(&work).await;
    commit_file(&work, "README.md", b"# hello\n", "initial").await;
    let out = run_git(&["push", "origin", "HEAD:refs/heads/main"], Some(&work)).await;
    assert!(out.status.success(), "seed push failed");

    // feature branch with one commit.
    run_git(&["checkout", "-b", "feature"], Some(&work)).await;
    commit_file(&work, "feature.txt", b"feature\n", "feature work").await;
    run_git(&["push", "origin", "feature"], Some(&work)).await;
    run_git(&["checkout", "main"], Some(&work)).await;

    // Tags on main: annotated v1.0.0, lightweight v0.9.
    run_git(&["tag", "-a", "v1.0.0", "-m", "release 1.0.0"], Some(&work)).await;
    run_git(&["tag", "v0.9"], Some(&work)).await;
    run_git(&["push", "origin", "--tags"], Some(&work)).await;

    // A branch that will be deleted later (prune test).
    run_git(&["checkout", "-b", "doomed"], Some(&work)).await;
    commit_file(&work, "doomed.txt", b"doomed\n", "doomed work").await;
    run_git(&["push", "origin", "doomed"], Some(&work)).await;
    run_git(&["checkout", "main"], Some(&work)).await;

    run_git(&["symbolic-ref", "HEAD", "refs/heads/main"], Some(&bare)).await;

    RichFixture {
        bare: bare.clone(),
        head_sha: rev_parse(&bare, "refs/heads/main").await,
        feature_sha: rev_parse(&bare, "refs/heads/feature").await,
        annotated_tag_sha: rev_parse(&bare, "v1.0.0^{commit}").await,
        annotated_tag_object: rev_parse(&bare, "v1.0.0").await,
        light_tag_sha: rev_parse(&bare, "v0.9").await,
    }
}

/// `git --git-dir=<bare> rev-parse <r>`; panics on failure.
pub async fn rev_parse(bare: &Path, r: &str) -> String {
    let out = run_git(&["--git-dir", bare.to_str().unwrap(), "rev-parse", r], None).await;
    assert!(out.status.success(), "rev-parse {r} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Clone a working copy from `bare` into `dest`.
pub async fn working_clone(bare: &Path, dest: &Path) {
    let out = run_git(
        &["clone", bare.to_str().unwrap(), dest.to_str().unwrap()],
        None,
    )
    .await;
    assert!(out.status.success(), "work clone failed");
    set_identity(dest).await;
}

/// Deterministic incompressible bytes (xorshift PRNG). Zlib cannot
/// meaningfully compress these, so a pack of them is ~their full size.
pub fn pseudo_random_bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// Standard base64 (for the expected `Authorization` header).
pub fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// A TCP server that accepts connections and never responds — Git
/// operations against it hang until cancelled or timed out.
pub struct HangingServer {
    pub addr: SocketAddr,
    _task: tokio::task::JoinHandle<()>,
}

pub async fn start_hanging_server() -> HangingServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _socket = socket;
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            });
        }
    });
    HangingServer { addr, _task: task }
}

/// A fake Git HTTP server: answers 401 (Basic) until a request
/// carries an `Authorization` header, which it captures, then answers
/// 200 with an empty body. Models a server that demands credentials.
pub struct AuthServer {
    pub addr: SocketAddr,
    pub captured_authorization: Arc<Mutex<Option<String>>>,
    _task: tokio::task::JoinHandle<()>,
}

pub async fn start_auth_server() -> AuthServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_task = captured.clone();
    let task = tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                break;
            };
            let captured = captured_task.clone();
            tokio::spawn(async move {
                let mut socket = socket;
                let mut buf = [0u8; 8192];
                loop {
                    let mut request = Vec::new();
                    // Read one HTTP request (headers only; GET has no body).
                    let read = async {
                        loop {
                            let n = socket.read(&mut buf).await.ok()?;
                            if n == 0 {
                                return None;
                            }
                            request.extend_from_slice(&buf[..n]);
                            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                                return Some(());
                            }
                        }
                    };
                    if read.await.is_none() {
                        return;
                    }
                    let text = String::from_utf8_lossy(&request);
                    let auth = text
                        .lines()
                        .find_map(|line| line.strip_prefix("Authorization: "))
                        .map(str::to_string);
                    if let Some(auth) = auth {
                        *captured.lock().await = Some(auth);
                        let _ = socket
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Type: application/x-git-upload-pack-advertisement\r\nContent-Length: 0\r\n\r\n",
                            )
                            .await;
                        return;
                    }
                    let _ = socket
                        .write_all(
                            b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"RepoNyx Test\"\r\nContent-Length: 0\r\n\r\n",
                        )
                        .await;
                }
            });
        }
    });
    AuthServer {
        addr,
        captured_authorization: captured,
        _task: task,
    }
}

/// Wait up to `timeout` for the auth server to capture an
/// `Authorization` header; returns it, or `None` on timeout.
pub async fn wait_for_authorization(
    server: &AuthServer,
    timeout: std::time::Duration,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(auth) = server.captured_authorization.lock().await.clone() {
            return Some(auth);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
