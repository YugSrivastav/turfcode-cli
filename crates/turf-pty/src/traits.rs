use crate::error::PtyError;

/// Trait for synchronously or non-blockingly reading chunks of bytes from a PTY stream.
pub trait PtyReadStream: Send {
    /// Read the next chunk of bytes, blocking until data arrives or stream ends.
    fn read_chunk(&mut self) -> Result<Option<Vec<u8>>, PtyError>;

    /// Try to read the next chunk of bytes without blocking.
    fn try_read_chunk(&mut self) -> Result<Option<Vec<u8>>, PtyError>;
}

/// Trait for writing bytes to a PTY stream.
pub trait PtyWriteStream: Send {
    /// Write raw bytes to the PTY.
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), PtyError>;

    /// Flush any pending writes to the PTY.
    fn flush(&mut self) -> Result<(), PtyError>;
}
