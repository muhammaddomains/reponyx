use reponyx_ipc::protocol::{
    ClientMessage, PROTOCOL_VERSION, ServerMessage, parse_client_message, parse_server_message,
};
use uuid::Uuid;

#[test]
fn ping_roundtrip() {
    let id = Uuid::new_v4();
    let msg = ClientMessage::Ping {
        protocol_version: PROTOCOL_VERSION,
        token: "secret".into(),
        request_id: id,
    };
    let bytes = serde_json::to_vec(&msg).unwrap();
    let parsed = parse_client_message(&bytes).unwrap();
    assert_eq!(parsed.request_id(), id);
    assert_eq!(parsed.protocol_version(), PROTOCOL_VERSION);
}

#[test]
fn reject_garbage() {
    assert!(parse_client_message(b"not-json").is_err());
}

#[test]
fn pong_roundtrip() {
    let id = Uuid::new_v4();
    let msg = ServerMessage::Pong {
        request_id: id,
        protocol_version: PROTOCOL_VERSION,
        agent_generation: 42,
    };
    let bytes = serde_json::to_vec(&msg).unwrap();
    let parsed = parse_server_message(&bytes).unwrap();
    match parsed {
        ServerMessage::Pong {
            request_id,
            agent_generation,
            ..
        } => {
            assert_eq!(request_id, id);
            assert_eq!(agent_generation, 42);
        }
        other => panic!("unexpected {other:?}"),
    }
}
