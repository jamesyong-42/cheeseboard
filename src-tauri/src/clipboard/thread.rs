use std::sync::mpsc;

/// Commands sent to the dedicated clipboard thread.
pub enum ClipboardCommand {
    /// Read the current clipboard text content.
    Read(mpsc::Sender<Option<String>>),
    /// Write text to the clipboard.
    Write(String),
    /// Shut down the clipboard thread.
    Shutdown,
}

/// Handle for communicating with the clipboard thread.
#[derive(Clone)]
pub struct ClipboardThread {
    tx: mpsc::Sender<ClipboardCommand>,
}

impl ClipboardThread {
    /// Spawn a dedicated OS thread for clipboard access.
    ///
    /// `arboard::Clipboard` must be created and used on a single thread
    /// (especially on macOS/Wayland). We use `std::thread` + `std::sync::mpsc`
    /// to keep it off the async runtime.
    pub fn spawn() -> Result<Self, ClipboardThreadError> {
        let (tx, rx) = mpsc::channel::<ClipboardCommand>();

        std::thread::Builder::new()
            .name("clipboard-io".into())
            .spawn(move || {
                let mut clipboard = match arboard::Clipboard::new() {
                    Ok(cb) => cb,
                    Err(e) => {
                        tracing::error!("Failed to create clipboard handle: {e}");
                        // Drain remaining commands so senders don't block
                        while let Ok(cmd) = rx.recv() {
                            match cmd {
                                ClipboardCommand::Read(reply) => {
                                    let _ = reply.send(None);
                                }
                                ClipboardCommand::Shutdown => break,
                                ClipboardCommand::Write(_) => {}
                            }
                        }
                        return;
                    }
                };

                tracing::info!("Clipboard thread started");

                loop {
                    match rx.recv() {
                        Ok(ClipboardCommand::Read(reply)) => {
                            let text = clipboard.get_text().ok();
                            let _ = reply.send(text);
                        }
                        Ok(ClipboardCommand::Write(text)) => {
                            if let Err(e) = clipboard.set_text(&text) {
                                tracing::warn!("Failed to write clipboard: {e}");
                            }
                        }
                        Ok(ClipboardCommand::Shutdown) => {
                            tracing::info!("Clipboard thread shutting down");
                            break;
                        }
                        Err(_) => {
                            // All senders dropped
                            tracing::info!("Clipboard thread: all senders dropped, exiting");
                            break;
                        }
                    }
                }
            })
            .map_err(|e| ClipboardThreadError::SpawnFailed(e.to_string()))?;

        Ok(Self { tx })
    }

    /// Read the current clipboard text. Blocks briefly waiting for the
    /// clipboard thread to respond.
    pub fn read(&self) -> Option<String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx.send(ClipboardCommand::Read(reply_tx)).ok()?;
        reply_rx.recv().ok().flatten()
    }

    /// Write text to the clipboard (non-blocking send to the thread).
    pub fn write(&self, text: String) {
        let _ = self.tx.send(ClipboardCommand::Write(text));
    }

    /// Signal the clipboard thread to shut down.
    pub fn shutdown(&self) {
        let _ = self.tx.send(ClipboardCommand::Shutdown);
    }
}

impl Drop for ClipboardThread {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardThreadError {
    #[error("Failed to spawn clipboard thread: {0}")]
    SpawnFailed(String),
}
