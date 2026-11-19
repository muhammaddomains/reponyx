//! Integration tests: credential helper injection.
//!
//! No real credentials anywhere: the token is a clearly fake value,
//! and the "server" is a local 127.0.0.1 fixture.

#[path = "common.rs"]
mod common;

use common::{base64_encode, start_auth_server, wait_for_authorization};
use reponyx_git_engine::commands::{GitCommand, build_ls_remote};
use reponyx_git_engine::credentials::{
    CREDENTIAL_PASSWORD_ENV, CREDENTIAL_USERNAME_ENV, Credential, CredentialHelper,
};
use reponyx_git_engine::process::{map_git_failure, run_git_command};
use std::path::PathBuf;
use std::time::Duration;

const FAKE_USER: &str = "test-user";
const FAKE_TOKEN: &str = "test-credential-token-12345";

fn git() -> PathBuf {
    PathBuf::from("git")
}

#[tokio::test]
async fn helper_serves_credentials_via_git_credential_fill() {
    let credential = Credential::new(FAKE_USER, FAKE_TOKEN).unwrap();
    let helper = CredentialHelper::create(&credential).unwrap();

    // The script file itself must not contain the secret.
    let script = std::fs::read_to_string(helper.script_path()).unwrap();
    assert!(!script.contains(FAKE_TOKEN));
    assert!(!script.contains(FAKE_USER));

    // Ask git to fill credentials for a host: the helper answers.
    let mut cmd = tokio::process::Command::new("git");
    cmd.args([
        "-c",
        &format!("credential.helper={}", helper.script_path().display()),
        "credential",
        "fill",
    ]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    cmd.env(CREDENTIAL_USERNAME_ENV, FAKE_USER);
    cmd.env(CREDENTIAL_PASSWORD_ENV, FAKE_TOKEN);
    let mut child = cmd.spawn().unwrap();
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().unwrap();
        stdin
            .write_all(b"protocol=https\nhost=example.com\npath=/o/r.git\n\n")
            .await
            .unwrap();
    }
    let out = child.wait_with_output().await.unwrap();
    assert!(out.status.success(), "credential fill failed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains(&format!("username={FAKE_USER}")));
    assert!(stdout.contains(&format!("password={FAKE_TOKEN}")));
}

#[tokio::test]
async fn ls_remote_sends_authorization_without_leaking_the_token() {
    let server = start_auth_server().await;
    let credential = Credential::new(FAKE_USER, FAKE_TOKEN).unwrap();
    let helper = CredentialHelper::create(&credential).unwrap();

    let url = format!("http://127.0.0.1:{}/repo.git", server.addr.port());
    let mut cmd = build_ls_remote(&git(), &url);
    helper.install(&mut cmd);
    // Keep proxies out of the way for localhost.
    let cmd = cmd.with_env("NO_PROXY", "127.0.0.1,localhost");
    let argv = cmd.argv().join(" ");
    assert!(!argv.contains(FAKE_TOKEN), "token must never be in argv");

    let (_tx, rx) = tokio::sync::watch::channel(false);
    let out = run_git_command(&cmd, Duration::from_secs(30), rx)
        .await
        .expect("ls-remote command failed to run");
    // The server returns an empty 200, so ls-remote itself fails —
    // that is fine. What matters is the Authorization header git sent.
    let auth = wait_for_authorization(&server, Duration::from_secs(10))
        .await
        .expect("server never saw an Authorization header");
    let expected = format!(
        "Basic {}",
        base64_encode(format!("{FAKE_USER}:{FAKE_TOKEN}").as_bytes())
    );
    assert_eq!(auth, expected);

    // And the token never appears in the captured process output.
    assert!(!out.stdout.contains(FAKE_TOKEN));
    assert!(!out.stderr.contains(FAKE_TOKEN));
}

#[tokio::test]
async fn helper_is_cleaned_up_after_a_failed_operation() {
    let credential = Credential::new(FAKE_USER, FAKE_TOKEN).unwrap();
    let helper = CredentialHelper::create(&credential).unwrap();
    let dir = helper.script_path().parent().unwrap().to_path_buf();
    assert!(dir.exists());

    let mut cmd = build_ls_remote(&git(), "http://127.0.0.1:1/nope.git");
    helper.install(&mut cmd);
    let (_tx, rx) = tokio::sync::watch::channel(false);
    let _ = run_git_command(&cmd, Duration::from_secs(10), rx).await;

    drop(helper);
    assert!(!dir.exists(), "helper dir must be removed on drop");
}

#[test]
fn failure_details_redact_the_token() {
    // If a token ever lands in git's stderr, the mapped error must
    // not carry it onwards.
    let cmd = GitCommand::new("git", vec!["ls-remote".into()]);
    let out = reponyx_git_engine::process::ProcessOutput {
        success: false,
        exit_code: Some(128),
        stdout: String::new(),
        stderr: format!(
            "fatal: Authentication failed for 'https://github.com/o/r.git/' password={FAKE_TOKEN}"
        ),
    };
    let err = map_git_failure(&cmd, &out);
    let details = err.details.unwrap_or_default();
    assert!(!details.contains(FAKE_TOKEN), "details leaked the token");
    assert!(details.contains("[REDACTED]"));
}
