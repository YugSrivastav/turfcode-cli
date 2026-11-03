use std::io::{Read, Write};
use std::thread;

use portable_pty::{native_pty_system, Child, ExitStatus, MasterPty, PtySize};
use tokio::sync::mpsc;

use crate::config::PtyOptions;
use crate::error::PtyError;
use crate::shell::build_command;
use crate::stream::{PtyReader, PtyWriteCommand, PtyWriter};

/// Handle representing the process lifecycle and sizing controls of a PTY.
pub struct PtyProcessHandle {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    rows: u16,
    cols: u16,
}

impl PtyProcessHandle {
    /// Resize the pseudo-terminal window.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), PtyError> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Resize(e.to_string()))?;
        self.rows = rows;
        self.cols = cols;
        Ok(())
    }

    /// Retrieve the current terminal dimensions as `(rows, cols)`.
    pub fn get_size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    /// Check if the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Attempt to check exit status without blocking.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, PtyError> {
        self.child
            .try_wait()
            .map_err(|e| PtyError::Process(e.to_string()))
    }

    /// Wait for the child process to terminate and obtain its exit status.
    pub fn wait(&mut self) -> Result<ExitStatus, PtyError> {
        self.child
            .wait()
            .map_err(|e| PtyError::Process(e.to_string()))
    }

    /// Terminate the child process immediately.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.child
            .kill()
            .map_err(|e| PtyError::Process(e.to_string()))
    }

    /// Process ID of the spawned child process, if available.
    pub fn pid(&self) -> Option<u32> {
        self.child.process_id()
    }
}

/// A fully managed PTY session encapsulating the master PTY, spawned child process,
/// and streaming reader and writer channels.
pub struct PtySession {
    handle: PtyProcessHandle,
    reader: PtyReader,
    writer: PtyWriter,
}

impl PtySession {
    /// Spawn a new PTY session with the given options.
    pub fn spawn(options: PtyOptions) -> Result<Self, PtyError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: options.rows,
                cols: options.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Allocation(e.to_string()))?;

        let cmd = build_command(&options);
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        // Drop the slave handle in the parent process so EOF behaves correctly.
        drop(pair.slave);

        let mut raw_reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::Allocation(format!("Failed to clone reader: {e}")))?;

        let mut raw_writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::Allocation(format!("Failed to take writer: {e}")))?;

        // Channel for asynchronous/synchronous streaming PTY output
        let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>(1024);
        thread::Builder::new()
            .name("turf-pty-reader-thread".to_string())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match raw_reader.read(&mut buffer) {
                        Ok(0) => {
                            // EOF reached
                            break;
                        }
                        Ok(n) => {
                            if out_tx.blocking_send(buffer[..n].to_vec()).is_err() {
                                // Downstream receiver disconnected
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::debug!("PTY reader stream finished: {e}");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| PtyError::Spawn(format!("Failed to spawn reader thread: {e}")))?;

        // Channel for asynchronous/synchronous streaming PTY input
        let (in_tx, mut in_rx) = mpsc::channel::<PtyWriteCommand>(1024);
        thread::Builder::new()
            .name("turf-pty-writer-thread".to_string())
            .spawn(move || {
                while let Some(command) = in_rx.blocking_recv() {
                    match command {
                        PtyWriteCommand::Write(bytes) => {
                            if let Err(e) = raw_writer.write_all(&bytes) {
                                tracing::error!("PTY write error: {e}");
                                break;
                            }
                            let _ = raw_writer.flush();
                        }
                        PtyWriteCommand::Flush(ack) => {
                            let result = raw_writer.flush().map_err(PtyError::from);
                            let _ = ack.send(result);
                        }
                    }
                }
            })
            .map_err(|e| PtyError::Spawn(format!("Failed to spawn writer thread: {e}")))?;

        let handle = PtyProcessHandle {
            master: pair.master,
            child,
            rows: options.rows,
            cols: options.cols,
        };

        Ok(Self {
            handle,
            reader: PtyReader::new(out_rx),
            writer: PtyWriter::new(in_tx),
        })
    }

    /// Convenience spawner for default platform shell at standard terminal dimensions.
    pub fn spawn_shell(rows: u16, cols: u16) -> Result<Self, PtyError> {
        Self::spawn(PtyOptions::new(rows, cols))
    }

    /// Resize the pseudo-terminal window.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), PtyError> {
        self.handle.resize(rows, cols)
    }

    /// Get current dimensions `(rows, cols)`.
    pub fn get_size(&self) -> (u16, u16) {
        self.handle.get_size()
    }

    /// Mutable reference to the PTY byte reader.
    pub fn reader_mut(&mut self) -> &mut PtyReader {
        &mut self.reader
    }

    /// Cloneable writer for dispatching bytes to the PTY.
    pub fn writer(&self) -> PtyWriter {
        self.writer.clone()
    }

    /// Write raw byte slice synchronously.
    pub fn write_bytes(&self, data: &[u8]) -> Result<(), PtyError> {
        self.writer.write_bytes_blocking(data)
    }

    /// Write a string synchronously.
    pub fn write_str(&self, text: &str) -> Result<(), PtyError> {
        self.write_bytes(text.as_bytes())
    }

    /// Write raw byte slice asynchronously.
    pub async fn write_bytes_async(&self, data: &[u8]) -> Result<(), PtyError> {
        self.writer.write_bytes_async(data).await
    }

    /// Write a string asynchronously.
    pub async fn write_str_async(&self, text: &str) -> Result<(), PtyError> {
        self.write_bytes_async(text.as_bytes()).await
    }

    /// Check if child process is running.
    pub fn is_alive(&mut self) -> bool {
        self.handle.is_alive()
    }

    /// Try waiting for exit status without blocking.
    pub fn try_wait(&mut self) -> Result<Option<ExitStatus>, PtyError> {
        self.handle.try_wait()
    }

    /// Wait for child process to terminate.
    pub fn wait(&mut self) -> Result<ExitStatus, PtyError> {
        self.handle.wait()
    }

    /// Kill child process.
    pub fn kill(&mut self) -> Result<(), PtyError> {
        self.handle.kill()
    }

    /// Child process ID.
    pub fn pid(&self) -> Option<u32> {
        self.handle.pid()
    }

    /// Split this session into an independent reader, writer, and process lifecycle handle.
    pub fn split(self) -> (PtyReader, PtyWriter, PtyProcessHandle) {
        (self.reader, self.writer, self.handle)
    }
}
