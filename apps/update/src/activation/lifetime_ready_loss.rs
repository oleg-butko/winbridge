//! Test-only partial delivery: consume the frame header, never decode its body.
//! The same stream remains owned; neither another send nor a new budget follows.
use super::Result;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt};

pub(crate) async fn read_header_only<S: AsyncRead + Unpin>(
    stream: &mut S,
    deadline: Instant,
) -> Result<usize> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or("Original lifetime Ready window expired before fault")?;
    let bytes = tokio::time::timeout(remaining, stream.read_u32_le())
        .await
        .map_err(|_| "Original lifetime Ready window expired during partial read")?
        .map_err(|_| "Private lifetime Ready header incomplete")? as usize;
    if bytes == 0 || bytes > super::channel::MAX_BYTES {
        return Err("Private lifetime Ready header length outside contract".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn partial_ready_consumes_only_header_and_preserves_same_stream_body() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        let body = br#"{"contractVersion":6,"receipt":"not decoded by the fault"}"#;
        writer.write_u32_le(body.len() as u32).await.unwrap();
        writer.write_all(body).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            read_header_only(&mut reader, deadline).await.unwrap(),
            body.len()
        );
        let mut remaining = vec![0; body.len()];
        reader.read_exact(&mut remaining).await.unwrap();
        assert_eq!(
            remaining, body,
            "Fault must not consume/validate the Ready body"
        );
    }

    #[tokio::test]
    async fn expired_original_window_consumes_nothing_and_cannot_be_renewed() {
        let (mut writer, mut reader) = tokio::io::duplex(32);
        writer.write_u32_le(1).await.unwrap();
        assert!(read_header_only(&mut reader, Instant::now()).await.is_err());
        assert_eq!(reader.read_u32_le().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn invalid_or_truncated_header_never_becomes_a_fault_success() {
        for length in [0, super::super::channel::MAX_BYTES as u32 + 1] {
            let (mut writer, mut reader) = tokio::io::duplex(32);
            writer.write_u32_le(length).await.unwrap();
            assert!(
                read_header_only(&mut reader, Instant::now() + Duration::from_secs(1))
                    .await
                    .is_err()
            );
        }
        let (mut writer, mut reader) = tokio::io::duplex(32);
        writer.write_all(&[1, 0]).await.unwrap();
        drop(writer);
        assert!(
            read_header_only(&mut reader, Instant::now() + Duration::from_secs(1))
                .await
                .unwrap_err()
                .contains("incomplete")
        );
    }
}
