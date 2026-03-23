use crate::error::{DockError, Result};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

// i3-ipc message types
pub(super) const MSG_RUN_COMMAND: u32 = 0;
pub(super) const MSG_SUBSCRIBE: u32 = 2;
pub(super) const MSG_GET_OUTPUTS: u32 = 3;
pub(super) const MSG_GET_TREE: u32 = 4;

pub(super) const I3_IPC_MAGIC: &[u8; 6] = b"i3-ipc";
pub(super) const HEADER_SIZE: usize = 14; // 6 (magic) + 4 (length) + 4 (type)

/// Maximum IPC response payload size (100MB safety cap).
const MAX_PAYLOAD_SIZE: usize = 100_000_000;

pub(super) fn send_message(conn: &mut UnixStream, msg_type: u32, payload: &[u8]) -> Result<()> {
    let mut message = Vec::with_capacity(HEADER_SIZE + payload.len());
    message.extend_from_slice(I3_IPC_MAGIC);
    message.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    message.extend_from_slice(&msg_type.to_le_bytes());
    message.extend_from_slice(payload);
    conn.write_all(&message)?;
    Ok(())
}

pub(super) fn read_response(conn: &mut UnixStream) -> Result<Vec<u8>> {
    let (body, _msg_type) = read_response_with_type(conn)?;
    Ok(body)
}

/// Reads an i3-ipc response, returning both payload and message type.
/// For event subscriptions, message type has bit 31 set (e.g., 0x80000000 = window,
/// 0x80000007 = output).
pub(super) fn read_response_with_type(conn: &mut UnixStream) -> Result<(Vec<u8>, u32)> {
    let mut header = [0u8; HEADER_SIZE];
    conn.read_exact(&mut header)?;

    // Validate magic
    if &header[..6] != I3_IPC_MAGIC {
        return Err(DockError::Ipc(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid i3-ipc magic in response",
        )));
    }

    let payload_len =
        u32::from_le_bytes(header[6..10].try_into().expect("slice is exactly 4 bytes")) as usize;
    let msg_type =
        u32::from_le_bytes(header[10..14].try_into().expect("slice is exactly 4 bytes"));
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(DockError::Ipc(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("IPC payload too large: {} bytes", payload_len),
        )));
    }
    let mut body = vec![0u8; payload_len];
    conn.read_exact(&mut body)?;
    Ok((body, msg_type))
}
