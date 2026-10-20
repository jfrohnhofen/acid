//! Minimal ANSI E1.31 ("streaming ACN" / sACN) data-packet parser.
//!
//! Only the "DMP data packet" is handled (the only packet type a DMX-over-sACN
//! receiver needs) - universe discovery and sync packets are out of scope.
//! Parsing is zero-copy: `DataPacket` borrows directly from the input buffer.
//!
//! Layout reference: ANSI E1.31-2018, built on the ACN root layer (E1.17).
//! Offsets below are the ones a well-formed, non-extended E1.31 packet always
//! uses (root layer vector = VECTOR_ROOT_E131_DATA, framing vector =
//! VECTOR_E131_DATA_PACKET, DMP vector = VECTOR_DMP_SET_PROPERTY).

const ACN_PACKET_IDENTIFIER: [u8; 12] = *b"ASC-E1.17\0\0\0";
const VECTOR_ROOT_E131_DATA: u32 = 0x0000_0004;
const VECTOR_E131_DATA_PACKET: u32 = 0x0000_0002;
const VECTOR_DMP_SET_PROPERTY: u8 = 0x02;
const DMP_ADDRESS_DATA_TYPE: u8 = 0xa1;

const OFF_ACN_PID: usize = 4;
const OFF_ROOT_VECTOR: usize = 18;
const OFF_CID: usize = 22;
const OFF_FRAMING_VECTOR: usize = 40;
const OFF_SOURCE_NAME: usize = 44;
const OFF_PRIORITY: usize = 108;
const OFF_SYNC_ADDRESS: usize = 109;
const OFF_SEQUENCE_NUMBER: usize = 111;
const OFF_OPTIONS: usize = 112;
const OFF_UNIVERSE: usize = 113;
const OFF_DMP_VECTOR: usize = 117;
const OFF_DMP_ADDRESS_DATA_TYPE: usize = 118;
const OFF_DMP_FIRST_PROPERTY_ADDRESS: usize = 119;
const OFF_DMP_ADDRESS_INCREMENT: usize = 121;
const OFF_DMP_PROPERTY_VALUE_COUNT: usize = 123;
const OFF_DMP_PROPERTY_VALUES: usize = 125;

/// Header size up to (and including) the property value count field, i.e.
/// the minimum length of any valid E1.31 data packet (start code only, no
/// DMX slots).
const MIN_PACKET_LEN: usize = OFF_DMP_PROPERTY_VALUES + 1;

const OPTIONS_STREAM_TERMINATED: u8 = 1 << 6;
const OPTIONS_PREVIEW_DATA: u8 = 1 << 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    TooShort,
    TooLong,
    BadAcnPacketIdentifier,
    BadRootVector,
    BadFramingVector,
    BadDmpVector,
    BadDmpAddressing,
    PropertyCountMismatch,
}

/// A parsed E1.31 DMP data packet, borrowing from the input buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataPacket<'a> {
    pub cid: &'a [u8; 16],
    pub source_name: &'a [u8; 64],
    pub priority: u8,
    pub sync_address: u16,
    pub sequence_number: u8,
    options: u8,
    pub universe: u16,
    /// DMX start code (should be 0x00 for standard "dimmer" data).
    pub start_code: u8,
    /// Up to 512 DMX slot values (channels 1..=512, in order), *excluding*
    /// the start code.
    pub dmx_data: &'a [u8],
}

impl<'a> DataPacket<'a> {
    pub fn is_preview_data(&self) -> bool {
        self.options & OPTIONS_PREVIEW_DATA != 0
    }

    pub fn is_stream_terminated(&self) -> bool {
        self.options & OPTIONS_STREAM_TERMINATED != 0
    }

    pub fn parse(buf: &'a [u8]) -> Result<Self, ParseError> {
        if buf.len() < MIN_PACKET_LEN {
            return Err(ParseError::TooShort);
        }

        if buf[OFF_ACN_PID..OFF_ACN_PID + 12] != ACN_PACKET_IDENTIFIER {
            return Err(ParseError::BadAcnPacketIdentifier);
        }

        let root_vector = u32::from_be_bytes(array4(buf, OFF_ROOT_VECTOR));
        if root_vector != VECTOR_ROOT_E131_DATA {
            return Err(ParseError::BadRootVector);
        }

        let framing_vector = u32::from_be_bytes(array4(buf, OFF_FRAMING_VECTOR));
        if framing_vector != VECTOR_E131_DATA_PACKET {
            return Err(ParseError::BadFramingVector);
        }

        if buf[OFF_DMP_VECTOR] != VECTOR_DMP_SET_PROPERTY {
            return Err(ParseError::BadDmpVector);
        }

        let first_property_address = u16::from_be_bytes(array2(buf, OFF_DMP_FIRST_PROPERTY_ADDRESS));
        let address_increment = u16::from_be_bytes(array2(buf, OFF_DMP_ADDRESS_INCREMENT));
        if buf[OFF_DMP_ADDRESS_DATA_TYPE] != DMP_ADDRESS_DATA_TYPE
            || first_property_address != 0
            || address_increment != 1
        {
            return Err(ParseError::BadDmpAddressing);
        }

        let property_value_count = u16::from_be_bytes(array2(buf, OFF_DMP_PROPERTY_VALUE_COUNT)) as usize;
        let available = buf.len() - OFF_DMP_PROPERTY_VALUES;
        if property_value_count == 0 || property_value_count > 513 || property_value_count > available {
            return Err(ParseError::PropertyCountMismatch);
        }

        let cid: &[u8; 16] = buf[OFF_CID..OFF_CID + 16].try_into().unwrap();
        let source_name: &[u8; 64] = buf[OFF_SOURCE_NAME..OFF_SOURCE_NAME + 64].try_into().unwrap();
        let priority = buf[OFF_PRIORITY];
        let sync_address = u16::from_be_bytes(array2(buf, OFF_SYNC_ADDRESS));
        let sequence_number = buf[OFF_SEQUENCE_NUMBER];
        let options = buf[OFF_OPTIONS];
        let universe = u16::from_be_bytes(array2(buf, OFF_UNIVERSE));
        let start_code = buf[OFF_DMP_PROPERTY_VALUES];
        let dmx_data = &buf[OFF_DMP_PROPERTY_VALUES + 1..OFF_DMP_PROPERTY_VALUES + property_value_count];

        Ok(DataPacket {
            cid,
            source_name,
            priority,
            sync_address,
            sequence_number,
            options,
            universe,
            start_code,
            dmx_data,
        })
    }
}

fn array2(buf: &[u8], off: usize) -> [u8; 2] {
    buf[off..off + 2].try_into().unwrap()
}

fn array4(buf: &[u8], off: usize) -> [u8; 4] {
    buf[off..off + 4].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a well-formed, minimal E1.31 data packet for `universe`
    /// carrying `dmx_data` (start code 0x00 is prepended automatically).
    fn build_packet(universe: u16, sequence_number: u8, priority: u8, dmx_data: &[u8]) -> Vec<u8> {
        let slot_count = dmx_data.len() + 1; // + start code
        let property_value_count = slot_count as u16;

        let mut buf = Vec::with_capacity(MIN_PACKET_LEN + dmx_data.len());

        // Root layer
        buf.extend_from_slice(&0x0010u16.to_be_bytes()); // preamble size
        buf.extend_from_slice(&0x0000u16.to_be_bytes()); // postamble size
        buf.extend_from_slice(&ACN_PACKET_IDENTIFIER);
        let root_len_flags = 0x7000 | ((buf.len() as u16 + 2 + 4 + 16 + 74) & 0x0FFF);
        buf.extend_from_slice(&root_len_flags.to_be_bytes()); // flags+length (not validated by parser)
        buf.extend_from_slice(&VECTOR_ROOT_E131_DATA.to_be_bytes());
        buf.extend_from_slice(&[0xAB; 16]); // CID

        // Framing layer
        buf.extend_from_slice(&0x7000u16.to_be_bytes()); // flags+length (not validated)
        buf.extend_from_slice(&VECTOR_E131_DATA_PACKET.to_be_bytes());
        let mut source_name = [0u8; 64];
        source_name[..4].copy_from_slice(b"test");
        buf.extend_from_slice(&source_name);
        buf.push(priority);
        buf.extend_from_slice(&0u16.to_be_bytes()); // sync address
        buf.push(sequence_number);
        buf.push(0); // options
        buf.extend_from_slice(&universe.to_be_bytes());

        // DMP layer
        buf.extend_from_slice(&0x7000u16.to_be_bytes()); // flags+length (not validated)
        buf.push(VECTOR_DMP_SET_PROPERTY);
        buf.push(DMP_ADDRESS_DATA_TYPE);
        buf.extend_from_slice(&0u16.to_be_bytes()); // first property address
        buf.extend_from_slice(&1u16.to_be_bytes()); // address increment
        buf.extend_from_slice(&property_value_count.to_be_bytes());
        buf.push(0x00); // DMX start code
        buf.extend_from_slice(dmx_data);

        assert_eq!(buf.len(), MIN_PACKET_LEN + dmx_data.len());
        buf
    }

    #[test]
    fn parses_well_formed_packet() {
        let data = [10u8, 20, 30, 40, 50];
        let buf = build_packet(42, 7, 100, &data);

        let pkt = DataPacket::parse(&buf).expect("should parse");
        assert_eq!(pkt.universe, 42);
        assert_eq!(pkt.sequence_number, 7);
        assert_eq!(pkt.priority, 100);
        assert_eq!(pkt.start_code, 0x00);
        assert_eq!(pkt.dmx_data, &data);
        assert!(!pkt.is_preview_data());
        assert!(!pkt.is_stream_terminated());
    }

    #[test]
    fn parses_full_512_channel_packet() {
        let data: Vec<u8> = (0..512u16).map(|i| (i % 256) as u8).collect();
        let buf = build_packet(1, 0, 100, &data);

        let pkt = DataPacket::parse(&buf).expect("should parse");
        assert_eq!(pkt.dmx_data.len(), 512);
        assert_eq!(pkt.dmx_data, &data[..]);
    }

    #[test]
    fn rejects_too_short_buffer() {
        let buf = [0u8; 10];
        assert_eq!(DataPacket::parse(&buf), Err(ParseError::TooShort));
    }

    #[test]
    fn rejects_bad_acn_packet_identifier() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf[4] = b'X';
        assert_eq!(DataPacket::parse(&buf), Err(ParseError::BadAcnPacketIdentifier));
    }

    #[test]
    fn rejects_bad_root_vector() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf[OFF_ROOT_VECTOR..OFF_ROOT_VECTOR + 4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert_eq!(DataPacket::parse(&buf), Err(ParseError::BadRootVector));
    }

    #[test]
    fn rejects_bad_framing_vector() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf[OFF_FRAMING_VECTOR..OFF_FRAMING_VECTOR + 4].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        assert_eq!(DataPacket::parse(&buf), Err(ParseError::BadFramingVector));
    }

    #[test]
    fn rejects_property_count_overrunning_buffer() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf[OFF_DMP_PROPERTY_VALUE_COUNT..OFF_DMP_PROPERTY_VALUE_COUNT + 2]
            .copy_from_slice(&600u16.to_be_bytes());
        assert_eq!(DataPacket::parse(&buf), Err(ParseError::PropertyCountMismatch));
    }

    #[test]
    fn extracts_preview_and_terminated_options() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf[OFF_OPTIONS] = OPTIONS_PREVIEW_DATA | OPTIONS_STREAM_TERMINATED;
        let pkt = DataPacket::parse(&buf).expect("should parse");
        assert!(pkt.is_preview_data());
        assert!(pkt.is_stream_terminated());
    }

    #[test]
    fn accepts_packet_with_trailing_padding() {
        let mut buf = build_packet(1, 0, 100, &[1, 2, 3]);
        buf.extend_from_slice(&[0u8; 100]); // 100 bytes of Ethernet frame padding
        let pkt = DataPacket::parse(&buf).expect("should parse despite trailing padding");
        assert_eq!(pkt.dmx_data, &[1, 2, 3]);
    }
}
