use reponyx_ipc::frame::{read_frame, write_frame};

#[tokio::test]
async fn frame_roundtrip() {
    let (client, server) = tokio::io::duplex(4096);
    let mut writer = client;
    let mut reader = server;

    let payload = br#"{"type":"ping"}"#;
    write_frame(&mut writer, payload).await.unwrap();
    let got = read_frame(&mut reader).await.unwrap();
    assert_eq!(got, payload);
}
