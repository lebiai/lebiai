//! Manual protobuf encode/decode for the Feishu WS frame protocol.
//!
//! The wire format is a tiny, fixed schema (2 messages, 10 fields total) that
//! hasn't changed in years. Rather than pulling in `prost` + `protobuf` + a
//! build step, we hand-roll the varint/length-delimited encoding. This is
//! straightforward because:
//! - All fields are either `uint64/varint`, `int32/varint`, `string/bytes`,
//!   or `repeated message`.
//! - Field numbers are stable (1–9 on Frame, 1–2 on Header).
//!
//! Reference: `oapi-sdk-go/ws/pbbp2.pb.go`.

// ---- wire-type constants --------------------------------------------------

const WT_VARINT: u8 = 0;
const WT_64BIT: u8 = 1;
const WT_LEN: u8 = 2;
// const WT_32BIT: u8 = 5; // not used

fn tag(field: u32, wt: u8) -> u64 {
    ((field as u64) << 3) | (wt as u64)
}

fn encode_varint(mut v: u64, buf: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            buf.push(b);
            return;
        }
        buf.push(b | 0x80);
    }
}

fn decode_varint(data: &[u8], pos: &mut usize) -> anyhow::Result<u64> {
    let mut v: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let b = data
            .get(*pos)
            .ok_or_else(|| anyhow::anyhow!("unexpected end of varint"))?;
        *pos += 1;
        v |= ((*b & 0x7F) as u64) << shift;
        if *b & 0x80 == 0 {
            return Ok(v);
        }
        shift += 7;
        if shift >= 64 {
            return Err(anyhow::anyhow!("varint overflow"));
        }
    }
}

fn decode_bytes(data: &[u8], pos: &mut usize) -> anyhow::Result<Vec<u8>> {
    let len = decode_varint(data, pos)? as usize;
    let end = *pos + len;
    if end > data.len() {
        return Err(anyhow::anyhow!("bytes field overflows buffer"));
    }
    let out = data[*pos..end].to_vec();
    *pos = end;
    Ok(out)
}

fn decode_string(data: &[u8], pos: &mut usize) -> anyhow::Result<String> {
    let raw = decode_bytes(data, pos)?;
    String::from_utf8(raw).map_err(|e| anyhow::anyhow!("invalid utf-8 in string field: {e}"))
}

// ---- Header ---------------------------------------------------------------

/// Key-value header in a Feishu WS frame.
/// Proto: `message Header { string key = 1; string value = 2; }`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub key: String,
    pub value: String,
}

impl Header {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        if !self.key.is_empty() {
            encode_varint(tag(1, WT_LEN), &mut buf);
            encode_varint(self.key.len() as u64, &mut buf);
            buf.extend_from_slice(self.key.as_bytes());
        }
        if !self.value.is_empty() {
            encode_varint(tag(2, WT_LEN), &mut buf);
            encode_varint(self.value.len() as u64, &mut buf);
            buf.extend_from_slice(self.value.as_bytes());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> anyhow::Result<Self> {
        let mut pos = 0;
        let mut key = String::new();
        let mut value = String::new();
        while pos < data.len() {
            let t = decode_varint(data, &mut pos)?;
            let field = (t >> 3) as u32;
            let wt = (t & 0x7) as u8;
            match (field, wt) {
                (1, WT_LEN) => key = decode_string(data, &mut pos)?,
                (2, WT_LEN) => value = decode_string(data, &mut pos)?,
                _ => {
                    // skip unknown
                    skip_field(wt, data, &mut pos)?;
                }
            }
        }
        Ok(Self { key, value })
    }
}

// ---- Frame ----------------------------------------------------------------

/// Top-level frame in the Feishu WS protocol.
///
/// Proto:
/// ```protobuf
/// message Frame {
///   uint64 SeqID           = 1;
///   uint64 LogID           = 2;
///   int32  service         = 3;
///   int32  method          = 4;
///   repeated Header headers = 5;
///   string payload_encoding = 6;
///   string payload_type     = 7;
///   bytes  payload          = 8;
///   string LogIDNew         = 9;
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub seq_id: u64,
    pub log_id: u64,
    pub service: i32,
    pub method: i32,
    pub headers: Vec<Header>,
    pub payload_encoding: String,
    pub payload_type: String,
    pub payload: Vec<u8>,
    pub log_id_new: String,
}

/// `method` values.
pub mod method {
    /// Control frame (ping/pong/handshake).
    pub const CONTROL: i32 = 0;
    /// Data frame (event/card).
    pub const DATA: i32 = 1;
}

/// Header key constants (mirrors Go SDK `ws/const.go`).
pub mod header_key {
    pub const TYPE: &str = "type";
    pub const MESSAGE_ID: &str = "message_id";
    pub const SUM: &str = "sum";
    pub const SEQ: &str = "seq";
    pub const TRACE_ID: &str = "trace_id";
    pub const BIZ_RT: &str = "biz_rt";
}

/// Header `type` values.
pub mod message_type {
    pub const EVENT: &str = "event";
    pub const CARD: &str = "card";
    pub const PING: &str = "ping";
    pub const PONG: &str = "pong";
}

impl Frame {
    /// Build a ping (control) frame.
    pub fn new_ping(service_id: i32) -> Self {
        Self {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: method::CONTROL,
            headers: vec![Header::new(header_key::TYPE, message_type::PING)],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        }
    }

    /// Look up a header value by key.
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.as_str())
    }

    /// Add or replace a header.
    pub fn set_header(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        if let Some(h) = self.headers.iter_mut().find(|h| h.key == key) {
            h.value = value.into();
        } else {
            self.headers.push(Header::new(key, value));
        }
    }

    /// Encode to protobuf bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64 + self.payload.len());
        // field 1: SeqID (varint)
        if self.seq_id != 0 {
            encode_varint(tag(1, WT_VARINT), &mut buf);
            encode_varint(self.seq_id, &mut buf);
        }
        // field 2: LogID (varint)
        if self.log_id != 0 {
            encode_varint(tag(2, WT_VARINT), &mut buf);
            encode_varint(self.log_id, &mut buf);
        }
        // field 3: service (varint, zigzag for sint32 — but proto says int32,
        // so we just encode as unsigned varint of the two's-complement repr)
        if self.service != 0 {
            encode_varint(tag(3, WT_VARINT), &mut buf);
            encode_varint(self.service as u64, &mut buf);
        }
        // field 4: method (varint)
        if self.method != 0 {
            encode_varint(tag(4, WT_VARINT), &mut buf);
            encode_varint(self.method as u64, &mut buf);
        }
        // field 5: headers (repeated length-delimited)
        for h in &self.headers {
            encode_varint(tag(5, WT_LEN), &mut buf);
            let hb = h.encode();
            encode_varint(hb.len() as u64, &mut buf);
            buf.extend_from_slice(&hb);
        }
        // field 6: payload_encoding
        if !self.payload_encoding.is_empty() {
            encode_varint(tag(6, WT_LEN), &mut buf);
            encode_varint(self.payload_encoding.len() as u64, &mut buf);
            buf.extend_from_slice(self.payload_encoding.as_bytes());
        }
        // field 7: payload_type
        if !self.payload_type.is_empty() {
            encode_varint(tag(7, WT_LEN), &mut buf);
            encode_varint(self.payload_type.len() as u64, &mut buf);
            buf.extend_from_slice(self.payload_type.as_bytes());
        }
        // field 8: payload (bytes)
        if !self.payload.is_empty() {
            encode_varint(tag(8, WT_LEN), &mut buf);
            encode_varint(self.payload.len() as u64, &mut buf);
            buf.extend_from_slice(&self.payload);
        }
        // field 9: LogIDNew
        if !self.log_id_new.is_empty() {
            encode_varint(tag(9, WT_LEN), &mut buf);
            encode_varint(self.log_id_new.len() as u64, &mut buf);
            buf.extend_from_slice(self.log_id_new.as_bytes());
        }
        buf
    }

    /// Decode from protobuf bytes.
    pub fn decode(data: &[u8]) -> anyhow::Result<Self> {
        let mut pos = 0;
        let mut seq_id = 0u64;
        let mut log_id = 0u64;
        let mut service = 0i32;
        let mut method = 0i32;
        let mut headers = Vec::new();
        let mut payload_encoding = String::new();
        let mut payload_type = String::new();
        let mut payload = Vec::new();
        let mut log_id_new = String::new();

        while pos < data.len() {
            let t = decode_varint(data, &mut pos)?;
            let field = (t >> 3) as u32;
            let wt = (t & 0x7) as u8;
            match (field, wt) {
                (1, WT_VARINT) => seq_id = decode_varint(data, &mut pos)?,
                (2, WT_VARINT) => log_id = decode_varint(data, &mut pos)?,
                (3, WT_VARINT) => service = decode_varint(data, &mut pos)? as i32,
                (4, WT_VARINT) => method = decode_varint(data, &mut pos)? as i32,
                (5, WT_LEN) => {
                    let raw = decode_bytes(data, &mut pos)?;
                    headers.push(Header::decode(&raw)?);
                }
                (6, WT_LEN) => payload_encoding = decode_string(data, &mut pos)?,
                (7, WT_LEN) => payload_type = decode_string(data, &mut pos)?,
                (8, WT_LEN) => payload = decode_bytes(data, &mut pos)?,
                (9, WT_LEN) => log_id_new = decode_string(data, &mut pos)?,
                _ => {
                    skip_field(wt, data, &mut pos)?;
                }
            }
        }

        Ok(Self {
            seq_id,
            log_id,
            service,
            method,
            headers,
            payload_encoding,
            payload_type,
            payload,
            log_id_new,
        })
    }
}

/// Skip an unknown field based on its wire type.
fn skip_field(wt: u8, data: &[u8], pos: &mut usize) -> anyhow::Result<()> {
    match wt {
        WT_VARINT => {
            decode_varint(data, pos)?;
        }
        WT_64BIT => {
            *pos += 8;
        }
        WT_LEN => {
            let len = decode_varint(data, pos)? as usize;
            *pos += len;
        }
        _ => {
            return Err(anyhow::anyhow!("unknown wire type {wt}"));
        }
    }
    if *pos > data.len() {
        return Err(anyhow::anyhow!("field overflows buffer"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ping_frame() {
        let frame = Frame::new_ping(42);
        let encoded = frame.encode();
        let decoded = Frame::decode(&encoded).unwrap();
        assert_eq!(decoded.method, method::CONTROL);
        assert_eq!(decoded.service, 42);
        assert_eq!(decoded.header(header_key::TYPE), Some(message_type::PING));
    }

    #[test]
    fn roundtrip_data_frame() {
        let mut frame = Frame {
            seq_id: 100,
            log_id: 200,
            service: 1,
            method: method::DATA,
            headers: vec![
                Header::new(header_key::TYPE, message_type::EVENT),
                Header::new(header_key::MESSAGE_ID, "msg-123"),
            ],
            payload_encoding: "json".to_string(),
            payload_type: String::new(),
            payload: br#"{"event":"im.message.receive_v1"}"#.to_vec(),
            log_id_new: String::new(),
        };
        frame.set_header(header_key::TRACE_ID, "trace-abc");
        let encoded = frame.encode();
        let decoded = Frame::decode(&encoded).unwrap();
        assert_eq!(decoded.seq_id, 100);
        assert_eq!(decoded.method, method::DATA);
        assert_eq!(decoded.header(header_key::TYPE), Some(message_type::EVENT));
        assert_eq!(decoded.header(header_key::MESSAGE_ID), Some("msg-123"));
        assert_eq!(decoded.header(header_key::TRACE_ID), Some("trace-abc"));
        assert_eq!(decoded.payload_encoding, "json");
        assert_eq!(
            String::from_utf8_lossy(&decoded.payload),
            r#"{"event":"im.message.receive_v1"}"#
        );
    }

    #[test]
    fn varint_roundtrip() {
        let values = [0u64, 1, 127, 128, 300, 1 << 20, u64::MAX];
        for &v in &values {
            let mut buf = Vec::new();
            encode_varint(v, &mut buf);
            let mut pos = 0;
            let decoded = decode_varint(&buf, &mut pos).unwrap();
            assert_eq!(v, decoded, "varint roundtrip failed for {v}");
        }
    }
}
