//! Short-lived WebSocket tickets so long-lived secrets need not appear in
//! `?token=` proxy access logs.
//!
//! Flow:
//! 1. Client `POST /api/v1/ws-ticket` with `Authorization: Bearer <token>`
//! 2. Server returns `{ "ticket": "<hex>", "expiresIn": 60 }`
//! 3. Client opens `ws://…/api/v1/chat?ticket=<hex>` (ticket is single-use, 60s)

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rand::RngCore;

const TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct TicketStore {
    inner: Mutex<HashMap<String, Instant>>,
}

impl TicketStore {
    pub fn issue(&self) -> String {
        let mut buf = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut buf);
        let ticket = hex_encode(&buf);
        let mut guard = self.inner.lock().expect("ticket store lock");
        // Opportunistic GC
        let now = Instant::now();
        guard.retain(|_, exp| *exp > now);
        guard.insert(ticket.clone(), now + TTL);
        ticket
    }

    /// Consume a ticket (single-use). Returns true if valid and not expired.
    pub fn consume(&self, ticket: &str) -> bool {
        let mut guard = self.inner.lock().expect("ticket store lock");
        let now = Instant::now();
        guard.retain(|_, exp| *exp > now);
        matches!(guard.remove(ticket), Some(exp) if exp > now)
    }

    pub fn ttl_secs() -> u64 {
        TTL.as_secs()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_consume_once() {
        let store = TicketStore::default();
        let t = store.issue();
        assert!(store.consume(&t));
        assert!(!store.consume(&t));
    }

    #[test]
    fn bad_ticket_rejected() {
        let store = TicketStore::default();
        assert!(!store.consume("nope"));
    }
}
