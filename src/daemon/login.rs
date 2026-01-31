use crate::core::models::Provider;
use crate::daemon::{DBUS_NAME, DBUS_PATH};
use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum LoginOutcome {
    Success,
    TimedOut,
    Failed(i32),
    MissingBinary,
    LaunchFailed(String),
}

#[derive(Debug)]
pub struct LoginResult {
    pub outcome: LoginOutcome,
    pub output: String,
    pub auth_link: Option<String>,
}

pub fn spawn_provider_login(provider: Provider) {
    std::thread::spawn(move || {
        let result = run_provider_login(provider);
        match &result.outcome {
            LoginOutcome::Success => {
                tracing::info!(?provider, "Login succeeded");
            }
            LoginOutcome::TimedOut => {
                tracing::warn!(?provider, "Login timed out");
            }
            LoginOutcome::MissingBinary => {
                tracing::warn!(?provider, "Login failed: CLI not found");
            }
            LoginOutcome::Failed(code) => {
                tracing::warn!(?provider, exit_code = *code, "Login failed");
            }
            LoginOutcome::LaunchFailed(message) => {
                tracing::warn!(?provider, error = %message, "Login launch failed");
            }
        }
        if !result.output.is_empty() {
            tracing::debug!(?provider, output_len = result.output.len(), "Login output captured");
        }
        if let Some(url) = result.auth_link.as_deref() {
            let _ = open::that(url);
        }
        if matches!(result.outcome, LoginOutcome::Success) {
            let _ = trigger_refresh();
        }
    });
}

fn run_provider_login(provider: Provider) -> LoginResult {
    match provider {
        Provider::Claude => run_claude_login(),
        Provider::Codex => run_codex_login(),
    }
}

fn run_claude_login() -> LoginResult {
    run_pty_login(
        "claude",
        &["/login"],
        Duration::from_secs(120),
        Duration::from_secs(1),
        &[
            "Successfully logged in",
            "Login successful",
            "Logged in successfully",
        ],
    )
}

fn run_codex_login() -> LoginResult {
    run_pty_login(
        "codex",
        &["login"],
        Duration::from_secs(120),
        Duration::from_secs(0),
        &["Logged in successfully", "Login successful"],
    )
}

fn run_pty_login(
    binary: &str,
    args: &[&str],
    timeout: Duration,
    send_enter_every: Duration,
    success_markers: &[&str],
) -> LoginResult {
    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: 50,
        cols: 160,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(e) => {
            return LoginResult {
                outcome: LoginOutcome::LaunchFailed(e.to_string()),
                output: String::new(),
                auth_link: None,
            }
        }
    };

    let mut cmd = CommandBuilder::new(binary);
    for arg in args {
        cmd.arg(arg);
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(e) => {
            let outcome = if e.to_string().to_lowercase().contains("not found") {
                LoginOutcome::MissingBinary
            } else {
                LoginOutcome::LaunchFailed(e.to_string())
            };
            return LoginResult {
                outcome,
                output: String::new(),
                auth_link: None,
            };
        }
    };

    let mut reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(e) => {
            return LoginResult {
                outcome: LoginOutcome::LaunchFailed(e.to_string()),
                output: String::new(),
                auth_link: None,
            }
        }
    };

    let mut writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(e) => {
            return LoginResult {
                outcome: LoginOutcome::LaunchFailed(e.to_string()),
                output: String::new(),
                auth_link: None,
            }
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let reader_handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = tx.send(buf[..n].to_vec());
                }
                Err(_) => break,
            }
        }
    });

    let start = Instant::now();
    let mut last_enter = Instant::now();
    let mut output = String::new();
    let mut auth_link: Option<String> = None;

    loop {
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = reader_handle.join();
            return LoginResult {
                outcome: LoginOutcome::TimedOut,
                output,
                auth_link,
            };
        }

        if send_enter_every > Duration::from_secs(0)
            && last_enter.elapsed() >= send_enter_every
        {
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
            last_enter = Instant::now();
        }

        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                if let Ok(text) = String::from_utf8(chunk) {
                    output.push_str(&text);
                    if output.len() > 8000 {
                        let drain = output.len() - 8000;
                        output.drain(..drain);
                    }
                    if auth_link.is_none() {
                        auth_link = first_link(&output);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if let Ok(Some(status)) = child.try_wait() {
            let _ = reader_handle.join();
            let outcome = if status.success() {
                LoginOutcome::Success
            } else {
                LoginOutcome::Failed(status.exit_code() as i32)
            };
            return LoginResult {
                outcome,
                output,
                auth_link,
            };
        }

        if success_markers.iter().any(|marker| output.contains(marker)) {
            let _ = child.kill();
            let _ = reader_handle.join();
            return LoginResult {
                outcome: LoginOutcome::Success,
                output,
                auth_link,
            };
        }
    }

    let _ = reader_handle.join();
    LoginResult {
        outcome: LoginOutcome::Failed(1),
        output,
        auth_link,
    }
}

fn first_link(text: &str) -> Option<String> {
    let mut best: Option<String> = None;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 7 < bytes.len() {
        let rest = &text[i..];
        let prefix = if rest.starts_with("https://") {
            "https://"
        } else if rest.starts_with("http://") {
            "http://"
        } else {
            i += 1;
            continue;
        };

        let mut end = i + prefix.len();
        while end < bytes.len() {
            let b = bytes[end];
            if b.is_ascii_whitespace() {
                break;
            }
            end += 1;
        }

        let mut url = text[i..end].to_string();
        while let Some(last) = url.chars().last() {
            if ".,;:)]}>\"'".contains(last) {
                url.pop();
            } else {
                break;
            }
        }

        if !url.is_empty() {
            best = Some(url);
            break;
        }
        i = end;
    }
    best
}

fn trigger_refresh() -> Result<()> {
    let connection = zbus::blocking::Connection::session()?;
    let _reply: () = connection
        .call_method(Some(DBUS_NAME), DBUS_PATH, Some(DBUS_NAME), "Refresh", &())?
        .body()
        .deserialize()?;
    Ok(())
}
