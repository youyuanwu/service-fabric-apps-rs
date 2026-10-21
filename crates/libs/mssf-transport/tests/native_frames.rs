use mssf_transport::{
    frame::Frame,
    message::{ListenInstance, Message},
    serialization::Decoder,
};

fn packet(hex: &str) -> Message {
    let bytes: Vec<u8> = hex
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
    Message::from_frame(Frame::decode(&bytes).unwrap()).unwrap()
}

#[test]
fn native_onebox_control_includes_supported_trailing_fields() {
    let message = packet(include_str!("fixtures/onebox-listen-normalized.hex"));
    let properties = message.properties().unwrap();
    assert_eq!(properties.actor, 1);
    assert_eq!(properties.action, "");
    assert!(properties.high_priority);
    let instance = ListenInstance::decode(&message.body).unwrap();
    assert_eq!(instance.address, "172.30.91.11:19000");
    assert_eq!(instance.instance, 0);
    assert_ne!(instance.nonce, [0; 16]);
}

#[test]
fn native_onebox_ping_has_independently_observed_node_identity() {
    let message = packet(include_str!("fixtures/onebox-ping-normalized.hex"));
    let properties = message.properties().unwrap();
    assert_eq!(properties.actor, 5);
    assert_eq!(properties.action, "PingRequest");
    assert_eq!(properties.relates_to.index, 2);
    assert_eq!(message.protocol_version().unwrap(), Some((1, 2)));
    let mut decoder = Decoder::new(&message.body);
    let (address, node_id, instance, name) = decoder
        .object(|decoder| {
            decoder.object(|decoder| {
                let address = decoder.string()?;
                let (node_id, instance) =
                    decoder.object(|decoder| Ok((decoder.byte_array()?, decoder.u64()?)))?;
                Ok((address, node_id, instance, decoder.string()?))
            })
        })
        .unwrap();
    decoder.finish().unwrap();
    assert_eq!(address, "172.30.91.11:19000");
    assert_eq!(name, "N0010");
    assert_eq!(instance, 134370832788957500);
    let mut expected = 0xc2e9_eff1_9761_acc9u64.to_le_bytes().to_vec();
    expected.extend_from_slice(&0x9244_22c5_3c89_43d0u64.to_le_bytes());
    assert_eq!(node_id, expected);
}
