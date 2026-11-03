use crate::error::PtyError;
use crate::traits::{PtyReadStream, PtyWriteStream};
use tokio::sync::{mpsc, oneshot};

/// Command dispatched to the PTY writer background worker.
pub enum PtyWriteCommand {
    Write(Vec<u8>),
    Flush(oneshot::Sender<Result<(), PtyError>>),
}

/// Task-safe and thread-safe PTY reader backed by an asynchronous mpsc channel.
pub struct PtyReader {
    rx: mpsc::Receiver<Vec<u8>>,
}

impl PtyReader {
    pub fn new(rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self { rx }
    }

    /// Asynchronously receive the next chunk of output from the PTY.
    pub async fn recv(&mut self) -> Option<Vec<u8>> {
        self.rx.recv().await
    }

    /// Synchronously block until the next chunk of output from the PTY arrives.
    pub fn blocking_recv(&mut self) -> Option<Vec<u8>> {
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| self.rx.blocking_recv())
        } else {
            self.rx.blocking_recv()
        }
    }

    /// Try to receive the next chunk without blocking.
    pub fn try_recv(&mut self) -> Result<Vec<u8>, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }
}

impl PtyReadStream for PtyReader {
    fn read_chunk(&mut self) -> Result<Option<Vec<u8>>, PtyError> {
        Ok(self.blocking_recv())
    }

    fn try_read_chunk(&mut self) -> Result<Option<Vec<u8>>, PtyError> {
        match self.try_recv() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => Ok(None),
        }
    }
}

/// Task-safe and thread-safe PTY writer backed by an asynchronous mpsc channel.
/// Cloning `PtyWriter` is cheap and provides multiple concurrent write handles.
#[derive(Clone)]
pub struct PtyWriter {
    tx: mpsc::Sender<PtyWriteCommand>,
}

impl PtyWriter {
    pub fn new(tx: mpsc::Sender<PtyWriteCommand>) -> Self {
        Self { tx }
    }

    /// Asynchronously send bytes to the PTY.
    pub async fn write_bytes_async(&self, data: &[u8]) -> Result<(), PtyError> {
        self.tx
            .send(PtyWriteCommand::Write(data.to_vec()))
            .await
            .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))
    }

    /// Synchronously send bytes to the PTY without panicking even within Tokio runtimes.
    pub fn write_bytes_blocking(&self, data: &[u8]) -> Result<(), PtyError> {
        let cmd = PtyWriteCommand::Write(data.to_vec());
        match self.tx.try_send(cmd) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(PtyError::Channel("PTY writer channel closed".to_string()))
            }
            Err(mpsc::error::TrySendError::Full(cmd)) => {
                if tokio::runtime::Handle::try_current().is_ok() {
                    tokio::task::block_in_place(|| {
                        self.tx
                            .blocking_send(cmd)
                            .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))
                    })
                } else {
                    self.tx
                        .blocking_send(cmd)
                        .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))
                }
            }
        }
    }

    /// Asynchronously request a flush of PTY writes.
    pub async fn flush_async(&self) -> Result<(), PtyError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.tx
            .send(PtyWriteCommand::Flush(ack_tx))
            .await
            .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))?;
        ack_rx
            .await
            .map_err(|_| PtyError::Channel("PTY writer flush dropped".to_string()))?
    }

    /// Synchronously block until pending PTY writes are flushed.
    pub fn flush_blocking(&self) -> Result<(), PtyError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        let cmd = PtyWriteCommand::Flush(ack_tx);
        let send_res = match self.tx.try_send(cmd) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(PtyError::Channel("PTY writer channel closed".to_string()))
            }
            Err(mpsc::error::TrySendError::Full(cmd)) => {
                if tokio::runtime::Handle::try_current().is_ok() {
                    tokio::task::block_in_place(|| {
                        self.tx
                            .blocking_send(cmd)
                            .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))
                    })
                } else {
                    self.tx
                        .blocking_send(cmd)
                        .map_err(|_| PtyError::Channel("PTY writer channel closed".to_string()))
                }
            }
        };
        send_res?;

        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::task::block_in_place(|| {
                ack_rx
                    .blocking_recv()
                    .map_err(|_| PtyError::Channel("PTY writer flush dropped".to_string()))?
            })
        } else {
            ack_rx
                .blocking_recv()
                .map_err(|_| PtyError::Channel("PTY writer flush dropped".to_string()))?
        }
    }
}

impl PtyWriteStream for PtyWriter {
    fn write_bytes(&mut self, data: &[u8]) -> Result<(), PtyError> {
        self.write_bytes_blocking(data)
    }

    fn flush(&mut self) -> Result<(), PtyError> {
        self.flush_blocking()
    }
}

impl std::io::Write for PtyWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_bytes_blocking(buf)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_blocking()
            .map_err(|e| std::io::Error::other(e.to_string()))
    }
}
