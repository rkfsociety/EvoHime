//! Запуск внешних программ без отдельного консольного окна с потоковой
//! передачей безопасного представления команды и её вывода.

use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Win32 `CREATE_NO_WINDOW`: консольная программа выполняется без создания
/// отдельного окна. Значение доступно на всех платформах для простого теста
/// контракта, но применяется только при сборке под Windows.
pub const WINDOWS_CREATION_FLAGS: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandEvent {
    Started {
        display: String,
    },
    Output {
        stream: CommandStream,
        line: String,
    },
    Finished {
        success: bool,
        exit_code: Option<i32>,
        elapsed: Duration,
    },
}

#[derive(Debug)]
pub struct ObservedCommandResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

/// Выполняет команду, публикуя только переданное вызывающей стороной
/// безопасное представление. Аргументы и окружение `Command` намеренно не
/// извлекаются автоматически, чтобы секреты не могли попасть в журнал.
pub async fn run_observed_command<F>(
    mut command: Command,
    safe_display: String,
    mut observer: F,
) -> io::Result<ObservedCommandResult>
where
    F: FnMut(CommandEvent),
{
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(WINDOWS_CREATION_FLAGS);

    let started = Instant::now();
    observer(CommandEvent::Started {
        display: safe_display,
    });

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            observer(CommandEvent::Finished {
                success: false,
                exit_code: None,
                elapsed: started.elapsed(),
            });
            return Err(error);
        }
    };

    let stdout = child
        .stdout
        .take()
        .expect("stdout is piped before the child process starts");
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped before the child process starts");
    let mut stdout_reader = BufReader::new(stdout);
    let mut stderr_reader = BufReader::new(stderr);
    let mut stdout_line = Vec::new();
    let mut stderr_line = Vec::new();
    let mut stdout_done = false;
    let mut stderr_done = false;
    let mut stdout_capture = String::new();
    let mut stderr_capture = String::new();
    let mut read_error = None;
    let wait_for_child = child.wait();
    tokio::pin!(wait_for_child);

    let status = loop {
        if stdout_done && stderr_done {
            match wait_for_child.await {
                Ok(status) => break status,
                Err(error) => {
                    observer(CommandEvent::Finished {
                        success: false,
                        exit_code: None,
                        elapsed: started.elapsed(),
                    });
                    return Err(error);
                }
            }
        }

        tokio::select! {
            result = &mut wait_for_child => {
                match result {
                    Ok(status) => break status,
                    Err(error) => {
                        observer(CommandEvent::Finished {
                            success: false,
                            exit_code: None,
                            elapsed: started.elapsed(),
                        });
                        return Err(error);
                    }
                }
            }
            read = stdout_reader.read_until(b'\n', &mut stdout_line), if !stdout_done => {
                match read {
                    Ok(0) => stdout_done = true,
                    Ok(_) => {
                        let line = decode_output_line(&stdout_line);
                        append_captured_line(&mut stdout_capture, &line);
                        observer(CommandEvent::Output {
                            stream: CommandStream::Stdout,
                            line,
                        });
                        stdout_line.clear();
                    }
                    Err(error) => {
                        read_error = Some(error);
                        break match wait_for_child.await {
                            Ok(status) => status,
                            Err(error) => {
                                observer(CommandEvent::Finished {
                                    success: false,
                                    exit_code: None,
                                    elapsed: started.elapsed(),
                                });
                                return Err(error);
                            }
                        };
                    }
                }
            }
            read = stderr_reader.read_until(b'\n', &mut stderr_line), if !stderr_done => {
                match read {
                    Ok(0) => stderr_done = true,
                    Ok(_) => {
                        let line = decode_output_line(&stderr_line);
                        append_captured_line(&mut stderr_capture, &line);
                        observer(CommandEvent::Output {
                            stream: CommandStream::Stderr,
                            line,
                        });
                        stderr_line.clear();
                    }
                    Err(error) => {
                        read_error = Some(error);
                        break match wait_for_child.await {
                            Ok(status) => status,
                            Err(error) => {
                                observer(CommandEvent::Finished {
                                    success: false,
                                    exit_code: None,
                                    elapsed: started.elapsed(),
                                });
                                return Err(error);
                            }
                        };
                    }
                }
            }
        }
    };

    if let Some(error) = read_error {
        observer(CommandEvent::Finished {
            success: false,
            exit_code: None,
            elapsed: started.elapsed(),
        });
        return Err(error);
    }
    let elapsed = started.elapsed();
    observer(CommandEvent::Finished {
        success: status.success(),
        exit_code: status.code(),
        elapsed,
    });

    Ok(ObservedCommandResult {
        status,
        stdout: stdout_capture,
        stderr: stderr_capture,
        elapsed,
    })
}

fn append_captured_line(output: &mut String, line: &str) {
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(line);
}

fn decode_output_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn streams_stdout_stderr_and_terminal_status() {
        let mut command = Command::new("powershell");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::Out.WriteLine('stdout-line'); [Console]::Error.WriteLine('stderr-line'); [Console]::Out.Flush(); [Console]::Error.Flush(); exit 7",
        ]);
        assert_failed_command_events(command).await;
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn streams_stdout_stderr_and_terminal_status() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf 'stdout-line\n'; printf 'stderr-line\n' >&2; exit 7",
        ]);
        assert_failed_command_events(command).await;
    }

    async fn assert_failed_command_events(command: Command) {
        let mut events = Vec::new();
        let result = run_observed_command(command, "safe command".to_string(), |event| {
            events.push(event);
        })
        .await
        .unwrap();

        assert!(matches!(
            events.first(),
            Some(CommandEvent::Started { display }) if display == "safe command"
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            CommandEvent::Output {
                stream: CommandStream::Stdout,
                line
            } if line == "stdout-line"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            CommandEvent::Output {
                stream: CommandStream::Stderr,
                line
            } if line == "stderr-line"
        )));
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(result.stdout, "stdout-line");
        assert_eq!(result.stderr, "stderr-line");
        assert!(matches!(
            events.last(),
            Some(CommandEvent::Finished { success: false, .. })
        ));
    }

    #[test]
    fn windows_commands_use_create_no_window() {
        assert_eq!(WINDOWS_CREATION_FLAGS, 0x0800_0000);
    }

    #[test]
    fn invalid_utf8_output_is_preserved_lossily() {
        assert_eq!(decode_output_line(&[b'o', b'k', 0xff, b'\r', b'\n']), "ok�");
    }
}
