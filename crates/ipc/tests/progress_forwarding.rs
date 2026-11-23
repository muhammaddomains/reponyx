//! Progress forwarding over IPC: a `ServerMessage::Progress` frame
//! pushed by the agent reaches a subscribed desktop client, and
//! normal request/response traffic on other connections is
//! unaffected by the push.

use reponyx_credential_store::memory_store;
use reponyx_ipc::{AgentIpcServer, IpcClient};
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn progress_reaches_subscriber_and_ping_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let store = memory_store();

    let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(64);
    let server = AgentIpcServer::bind(dir.path(), &store, 1, cmd_tx)
        .await
        .unwrap();

    // The queue's JSON progress channel (the agent wires the real
    // queue broadcast through this shape).
    let (json_tx, json_rx) = broadcast::channel::<serde_json::Value>(16);
    server.set_progress_source(json_rx).await;

    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();
    let server_handle = tokio::spawn(async move {
        server.run(server_cancel).await;
    });

    let client = IpcClient::open(dir.path(), &store).unwrap();
    let mut progress = client.subscribe_progress().await.unwrap();

    // A normal request on its own connection still gets its reply
    // while the subscription is open.
    let (generation, _protocol) = client.ping().await.unwrap();
    assert_eq!(generation, 1);

    let event = json!({
        "type": "job_progress",
        "job_id": "00000000-0000-0000-0000-000000000001",
        "stage": "receiving objects",
        "detail": "Receiving objects: 45%",
        "percent": 45,
    });
    json_tx.send(event.clone()).unwrap();

    let received = tokio::time::timeout(Duration::from_secs(5), progress.recv())
        .await
        .expect("progress event did not arrive in time")
        .unwrap();
    assert_eq!(received, event);

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}

#[tokio::test]
async fn progress_without_source_keeps_requests_working() {
    // A server with no progress source attached must still serve
    // requests — the forwarder simply has nothing to forward.
    let dir = tempfile::tempdir().unwrap();
    let store = memory_store();

    let (cmd_tx, _cmd_rx) = tokio::sync::mpsc::channel(64);
    let server = AgentIpcServer::bind(dir.path(), &store, 1, cmd_tx)
        .await
        .unwrap();

    let cancel = CancellationToken::new();
    let server_cancel = cancel.clone();
    let server_handle = tokio::spawn(async move {
        server.run(server_cancel).await;
    });

    let client = IpcClient::open(dir.path(), &store).unwrap();
    let (generation, _protocol) = client.ping().await.unwrap();
    assert_eq!(generation, 1);

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;
}
