//! A single Git process, with one request/body outstanding at a time.
use crate::{error, transfer, Result};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};
use winbridge_core::snapshot::MAX_UNPACKED_BYTES;

fn header(reader: &mut impl BufRead, object: &str) -> Result<u64> {
    let mut bytes = Vec::new();
    reader
        .take(256)
        .read_until(b'\n', &mut bytes)
        .map_err(error)?;
    if bytes.last() != Some(&b'\n') {
        return Err("Git batch header truncated or exceeds 256 bytes".into());
    }
    let line = std::str::from_utf8(&bytes[..bytes.len() - 1]).map_err(error)?;
    let fields: Vec<_> = line.split(' ').collect();
    if fields.len() != 3 || fields[0] != object || fields[1] != "blob" {
        return Err("Git batch object/type mismatch or missing object".into());
    }
    let size = fields[2].parse::<u64>().map_err(error)?;
    if size > MAX_UNPACKED_BYTES {
        return Err("Git blob exceeds unpacked limit".into());
    }
    Ok(size)
}

fn body(reader: &mut impl Read, writer: &mut impl Write, size: u64) -> Result<String> {
    let digest = transfer(&mut (&mut *reader).take(size), writer, size, None, |_| {
        Ok(())
    })?;
    let mut separator = [0];
    reader.read_exact(&mut separator).map_err(error)?;
    if separator != *b"\n" {
        return Err("Git batch blob separator mismatch".into());
    }
    Ok(digest)
}

pub(crate) struct GitBatch {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    pending_size: Option<u64>,
}
impl GitBatch {
    pub fn open(root: &Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("--no-replace-objects")
            .arg("-C")
            .arg(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Git batch unavailable: {e}"))?;
        // Stdio::piped guarantees these handles on successful spawn.
        let input = child.stdin.take().expect("piped Git stdin");
        let output = BufReader::new(child.stdout.take().expect("piped Git stdout"));
        Ok(Self {
            child,
            input: Some(input),
            output,
            pending_size: None,
        })
    }
    pub fn size(&mut self, object: &str) -> Result<u64> {
        if self.pending_size.is_some()
            || !matches!(object.len(), 40 | 64)
            || !object.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Git batch requires a valid object and the previous body drained".into());
        }
        let input = self.input.as_mut().ok_or("Git batch closed")?;
        writeln!(input, "{object}")
            .and_then(|_| input.flush())
            .map_err(error)?;
        let size = header(&mut self.output, object)?;
        self.pending_size = Some(size);
        Ok(size)
    }
    pub fn copy_blob(&mut self, writer: &mut impl Write) -> Result<String> {
        let size = self
            .pending_size
            .take()
            .ok_or("Git batch has no pending body")?;
        // A limited reader supplies EOF to transfer without consuming the next
        // Git framing byte. Binary data (including newlines/NUL) is never parsed.
        body(&mut self.output, writer, size)
    }
    pub fn finish(mut self) -> Result<()> {
        if self.pending_size.is_some() {
            return Err("Git batch closed with an unread body".into());
        }
        drop(self.input.take());
        let mut extra = [0];
        if self.output.read(&mut extra).map_err(error)? != 0 {
            return Err("Unexpected trailing Git batch bytes".into());
        }
        if !self.child.wait().map_err(error)?.success() {
            return Err("Git batch capture failed".into());
        }
        Ok(())
    }
}
impl Drop for GitBatch {
    fn drop(&mut self) {
        // Also covers failed ZIP writes, bad headers and early size-limit exits.
        // Child::drop alone would leave a process waiting for more stdin.
        drop(self.input.take());
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_headers_refuse_missing_wrong_types_truncation_and_size_overflow() {
        let oid = "a".repeat(40);
        for line in [
            format!("{oid} missing\n"),
            format!("{oid} tree 1\n"),
            format!("{} blob 1\n", "b".repeat(40)),
            format!("{oid} blob 1"),
            format!("{oid} blob {}\n", MAX_UNPACKED_BYTES + 1),
            format!("{oid} blob {}\n", "9".repeat(512)),
        ] {
            let mut input = std::io::Cursor::new(line);
            assert!(header(&mut input, &oid).is_err());
            assert!(input.position() <= 256);
        }
        assert_eq!(
            header(&mut std::io::Cursor::new(format!("{oid} blob 0\n")), &oid).unwrap(),
            0
        );
    }
    #[test]
    fn binary_bodies_are_exactly_framed_and_truncated_or_wrong_separators_fail() {
        let bytes = b"\0\nblob\xff\n";
        let mut input = std::io::Cursor::new([bytes.as_slice(), b"\nnext header\n"].concat());
        let mut output = Vec::new();
        assert_eq!(
            body(&mut input, &mut output, bytes.len() as u64).unwrap(),
            crate::sha256(bytes)
        );
        assert_eq!(output, bytes);
        assert_eq!(input.position(), bytes.len() as u64 + 1);
        assert_eq!(
            body(&mut std::io::Cursor::new(b"\nnext"), &mut Vec::new(), 0).unwrap(),
            crate::sha256(b"")
        );
        for input in [b"abc".as_slice(), b"abcd", b"abcdX"] {
            assert!(body(&mut std::io::Cursor::new(input), &mut Vec::new(), 4).is_err());
        }
    }
}
