//! Serial consoles (console cables, USB-serial adapters): a session actor over a local
//! serial port with the same `SessionHandle` protocol as the other sessions. Port I/O is
//! blocking, so reads and writes run on their own threads, like the PTY.

use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hopr_core::model::{Parity, SerialConfig, SerialFlow};
use hopr_core::session::{COMMAND_CAPACITY, FlowControl, SessionCommand, SessionHandle};
use hopr_core::{AppError, CloseReason, ErrorCode, SessionId, SessionState};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info};

/// Serial ports on this machine (names to put in a serial host's "port" field).
pub fn serial_ports() -> Vec<String> {
    let mut names: Vec<String> = serialport::available_ports()
        .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default();
    names.sort();
    names.dedup();
    names
}

fn open_error(path: &str, e: &serialport::Error) -> AppError {
    let message = match e.kind() {
        serialport::ErrorKind::NoDevice => format!("{path} isn't connected"),
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
            format!("{path} is in use by another program, or access was denied")
        }
        _ => format!("Couldn't open {path}"),
    };
    AppError::new(ErrorCode::ConnectFailed, message).with_detail(e.to_string())
}

fn builder(path: &str, cfg: &SerialConfig) -> serialport::SerialPortBuilder {
    serialport::new(path, cfg.baud)
        .data_bits(match cfg.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        })
        .parity(match cfg.parity {
            Parity::None => serialport::Parity::None,
            Parity::Odd => serialport::Parity::Odd,
            Parity::Even => serialport::Parity::Even,
        })
        .stop_bits(if cfg.stop_bits == 2 {
            serialport::StopBits::Two
        } else {
            serialport::StopBits::One
        })
        .flow_control(match cfg.flow {
            SerialFlow::None => serialport::FlowControl::None,
            SerialFlow::Software => serialport::FlowControl::Software,
            SerialFlow::Hardware => serialport::FlowControl::Hardware,
        })
        // Short reads so the reader thread notices a close quickly.
        .timeout(Duration::from_millis(100))
}

/// Open a serial session. Fails at once if the port can't be opened.
pub fn spawn_serial(
    id: SessionId,
    path: &str,
    cfg: &SerialConfig,
    output: mpsc::Sender<Vec<u8>>,
    flow: Arc<FlowControl>,
) -> Result<SessionHandle, AppError> {
    cfg.validate()?;
    let port = builder(path, cfg)
        .open()
        .map_err(|e| open_error(path, &e))?;
    let mut reader = port.try_clone().map_err(|e| open_error(path, &e))?;
    let mut writer = port;
    let stop = Arc::new(AtomicBool::new(false));

    let (lost_tx, mut lost_rx) = mpsc::channel::<()>(1);
    let reader_stop = stop.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8 * 1024];
        while !reader_stop.load(Ordering::Relaxed) {
            match reader.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    if output.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(_) => {
                    // Unplugged or the driver gave up.
                    let _ = lost_tx.blocking_send(());
                    break;
                }
            }
        }
    });
    let (write_tx, write_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(COMMAND_CAPACITY);
    std::thread::spawn(move || {
        while let Ok(data) = write_rx.recv() {
            if writer
                .write_all(&data)
                .and_then(|()| writer.flush())
                .is_err()
            {
                break;
            }
        }
    });

    let (cmd_tx, mut cmd_rx) = mpsc::channel(COMMAND_CAPACITY);
    let (state_tx, state_rx) = watch::channel(SessionState::Ready);
    let span = tracing::info_span!("serial", session_id = %id, port = %path);
    tokio::spawn(
        async move {
            let end = loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => match cmd {
                        Some(SessionCommand::Write(data)) => {
                            if write_tx.send(data).is_err() {
                                break CloseReason::ConnectionLost;
                            }
                        }
                        // A serial line has no window size.
                        Some(SessionCommand::Resize { .. } | SessionCommand::HostKeyDecision(_) | SessionCommand::PromptResponse(_)) => {}
                        Some(SessionCommand::Close) | None => break CloseReason::UserClosed,
                    },
                    _ = lost_rx.recv() => break CloseReason::ConnectionLost,
                }
            };
            stop.store(true, Ordering::Relaxed);
            info!(?end, "serial session ended");
            state_tx.send_replace(SessionState::Closed { reason: end });
        }
        .instrument(span),
    );
    Ok(SessionHandle::new(id, cmd_tx, state_rx, flow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_port_is_a_friendly_error() {
        let (tx, _rx) = mpsc::channel(4);
        let Err(e) = spawn_serial(
            "s".into(),
            "COM-HOPR-NONE",
            &SerialConfig::default(),
            tx,
            Arc::new(FlowControl::default()),
        ) else {
            panic!("a port that doesn't exist must not open");
        };
        assert_eq!(e.code, ErrorCode::ConnectFailed);
        assert!(e.message.contains("COM-HOPR-NONE"), "{}", e.message);
    }

    #[test]
    fn bad_settings_are_refused_before_opening() {
        let (tx, _rx) = mpsc::channel(4);
        let cfg = SerialConfig {
            data_bits: 9,
            ..SerialConfig::default()
        };
        let Err(e) = spawn_serial(
            "s".into(),
            "COM1",
            &cfg,
            tx,
            Arc::new(FlowControl::default()),
        ) else {
            panic!("9 data bits must be refused");
        };
        assert_eq!(e.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn lists_ports_without_failing() {
        // May be empty on CI; it must not panic or error.
        let ports = serial_ports();
        assert!(ports.windows(2).all(|w| w[0] < w[1]));
    }
}
