use rosc::{OscBundle, OscError, OscMessage, OscPacket};

pub(crate) const MTU: usize = 1472;
const HEADER_SIZE: usize = 16;
const ELEMENT_SIZE: usize = 4;

pub(crate) struct Packed {
    pub(crate) packets: Vec<OscPacket>,
    pub(crate) oversize: usize,
}

/// Greedy, immediate bundles in input priority order. Messages too large for
/// an empty bundle go alone, even when the plain message itself exceeds MTU.
pub(crate) fn pack(
    messages: impl IntoIterator<Item = OscMessage>,
    mtu: usize,
) -> Result<Packed, OscError> {
    let mut packed = Packed {
        packets: Vec::new(),
        oversize: 0,
    };
    let mut content = Vec::new();
    let mut size = HEADER_SIZE;
    for message in messages {
        let packet = OscPacket::Message(message);
        let element_size = ELEMENT_SIZE + rosc::encoder::encode(&packet)?.len();
        if size + element_size > mtu {
            flush(&mut packed.packets, &mut content);
            size = HEADER_SIZE;
        }
        if HEADER_SIZE + element_size > mtu {
            packed.packets.push(packet);
            packed.oversize += 1;
        } else {
            content.push(packet);
            size += element_size;
        }
    }
    flush(&mut packed.packets, &mut content);
    Ok(packed)
}

fn flush(packets: &mut Vec<OscPacket>, content: &mut Vec<OscPacket>) {
    if !content.is_empty() {
        packets.push(OscPacket::Bundle(OscBundle {
            timetag: (0, 1).into(),
            content: std::mem::take(content),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosc::OscType;

    fn message(key: i32, bytes: usize) -> OscMessage {
        OscMessage {
            addr: "/test".into(),
            args: vec![OscType::Int(key), OscType::Blob(vec![0; bytes])],
        }
    }

    fn encoded_size(packet: &OscPacket) -> usize {
        rosc::encoder::encode(packet).unwrap().len()
    }

    fn assert_bundle(packet: &OscPacket, mtu: usize) {
        let OscPacket::Bundle(bundle) = packet else {
            panic!("expected a bundle");
        };
        assert_eq!(bundle.timetag, (0, 1).into());
        assert!(!bundle.content.is_empty());
        assert!(encoded_size(packet) <= mtu);
    }

    #[test]
    fn exactly_full_bundle_and_one_byte_over_limit() {
        let small = message(1, 0);
        let small_size = encoded_size(&OscPacket::Message(small.clone())) + ELEMENT_SIZE;
        let empty_size = encoded_size(&OscPacket::Message(message(2, 0))) + ELEMENT_SIZE;
        let large = message(2, MTU - HEADER_SIZE - small_size - empty_size);
        let packed = pack([small.clone(), large.clone()], MTU).unwrap();
        assert_eq!(packed.packets.len(), 1);
        assert_eq!(encoded_size(&packed.packets[0]), MTU);
        assert_bundle(&packed.packets[0], MTU);
        assert_eq!(packed.oversize, 0);

        // OSC sizes are multiples of four; lower the limit one byte to cross it.
        let packed = pack([small, large], MTU - 1).unwrap();
        assert_eq!(packed.packets.len(), 2);
        assert_eq!(packed.oversize, 0);
        for packet in &packed.packets {
            assert_bundle(packet, MTU - 1);
        }
    }

    #[test]
    fn oversize_messages_go_alone_without_reordering() {
        let messages = [message(1, 0), message(2, MTU), message(3, 0)];
        let packed = pack(messages.clone(), MTU).unwrap();
        assert_eq!(packed.packets.len(), 3);
        assert_eq!(packed.oversize, 1);
        assert_bundle(&packed.packets[0], MTU);
        assert_eq!(packed.packets[1], OscPacket::Message(messages[1].clone()));
        assert!(encoded_size(&packed.packets[1]) > MTU);
        assert_bundle(&packed.packets[2], MTU);
        assert_eq!(flatten(packed.packets), messages);
    }

    #[test]
    fn message_that_only_exceeds_bundle_overhead_goes_plain() {
        let base = encoded_size(&OscPacket::Message(message(1, 0)));
        let message = message(1, MTU - base);
        assert_eq!(encoded_size(&OscPacket::Message(message.clone())), MTU);
        let packed = pack([message.clone()], MTU).unwrap();
        assert_eq!(packed.packets, [OscPacket::Message(message)]);
        assert_eq!(packed.oversize, 1);
    }

    fn flatten(packets: Vec<OscPacket>) -> Vec<OscMessage> {
        packets
            .into_iter()
            .flat_map(|packet| match packet {
                OscPacket::Message(message) => vec![message],
                OscPacket::Bundle(bundle) => bundle
                    .content
                    .into_iter()
                    .map(|packet| {
                        let OscPacket::Message(message) = packet else {
                            panic!("nested bundle");
                        };
                        message
                    })
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn priority_order_survives_multiple_bundles() {
        let messages: Vec<_> = (0..20).rev().map(|key| message(key, 100)).collect();
        let packed = pack(messages.clone(), MTU).unwrap();
        assert!(packed.packets.len() > 1);
        for packet in &packed.packets {
            assert_bundle(packet, MTU);
        }
        assert_eq!(flatten(packed.packets), messages);
    }

    #[test]
    fn one_message_round_trips_with_immediate_timetag() {
        let message = message(7, 3);
        let packed = pack([message.clone()], MTU).unwrap();
        assert_eq!(packed.packets.len(), 1);
        let bytes = rosc::encoder::encode(&packed.packets[0]).unwrap();
        let (rest, decoded) = rosc::decoder::decode_udp(&bytes).unwrap();
        assert!(rest.is_empty());
        assert_bundle(&decoded, MTU);
        assert_eq!(flatten(vec![decoded]), [message]);
    }

    #[test]
    fn empty_input_produces_no_packet() {
        let packed = pack([], MTU).unwrap();
        assert!(packed.packets.is_empty());
        assert_eq!(packed.oversize, 0);
    }
}
