use crate::job::Job;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Out,
    Err,
}

pub struct Cmd {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub envs: Vec<(String, String)>,
}

impl Cmd {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            envs: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    pub fn display(&self) -> String {
        let mut s = self.program.display().to_string();
        for a in &self.args {
            s.push(' ');
            if a.contains(' ') {
                s.push('"');
                s.push_str(a);
                s.push('"');
            } else {
                s.push_str(a);
            }
        }
        if s.chars().count() > 500 {
            s = s.chars().take(500).collect::<String>() + "…";
        }
        s
    }
}

pub struct Outcome {
    pub status: Option<ExitStatus>,
    pub stdout: String,
    pub stderr: String,
    pub cancelled: bool,
}

impl Outcome {
    pub fn success(&self) -> bool {
        self.status.map(|s| s.success()).unwrap_or(false) && !self.cancelled
    }

    pub fn code(&self) -> Option<i32> {
        self.status.and_then(|s| s.code())
    }

    /// Compact, human readable error text built from the tail of stderr.
    pub fn error_text(&self) -> String {
        let lines: Vec<&str> = self
            .stderr
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        let tail = lines
            .iter()
            .rev()
            .take(8)
            .rev()
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        if tail.is_empty() {
            match self.code() {
                Some(code) => format!("終了コード {code} で失敗しました"),
                None => "処理に失敗しました".to_string(),
            }
        } else {
            tail
        }
    }
}

fn build_command(cmd: &Cmd) -> Command {
    let mut command = Command::new(&cmd.program);
    command
        .args(&cmd.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &cmd.cwd {
        command.current_dir(cwd);
    }
    for (k, v) in &cmd.envs {
        command.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

pub fn run_capture(cmd: &Cmd) -> Result<Outcome, String> {
    let output = build_command(cmd)
        .output()
        .map_err(|e| format!("{} を起動できません: {e}", cmd.program.display()))?;
    Ok(Outcome {
        status: Some(output.status),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        cancelled: false,
    })
}

/// Runs a process while streaming stdout/stderr lines to `on_line`.
/// The child is registered with the job so that cancel_job() can kill it.
pub fn run_streaming<F>(job: &Job, cmd: &Cmd, mut on_line: F) -> Result<Outcome, String>
where
    F: FnMut(Stream, &str),
{
    job.check()?;
    let mut child = build_command(cmd)
        .spawn()
        .map_err(|e| format!("{} を起動できません: {e}", cmd.program.display()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = mpsc::channel::<(Stream, String)>();

    if let Some(out) = stdout {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(out);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = buf.trim_end_matches(['\n', '\r']).to_string();
                        if tx.send((Stream::Out, line)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    if let Some(err) = stderr {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(err);
            let mut buf = String::new();
            loop {
                buf.clear();
                match reader.read_line(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = buf.trim_end_matches(['\n', '\r']).to_string();
                        if tx.send((Stream::Err, line)).is_err() {
                            break;
                        }
                    }
                }
            }
        });
    }
    drop(tx);

    let child = Arc::new(Mutex::new(child));
    job.attach_child(child.clone());

    let mut stdout_acc = String::new();
    let mut stderr_tail: VecDeque<String> = VecDeque::with_capacity(160);
    let mut status: Option<ExitStatus> = None;
    let mut cancelled = false;

    loop {
        if job.is_cancelled() {
            cancelled = true;
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
            break;
        }

        // Drain available output without holding the child lock.
        while let Ok((stream, line)) = rx.try_recv() {
            match stream {
                Stream::Out => {
                    if stdout_acc.len() < 8 * 1024 * 1024 {
                        stdout_acc.push_str(&line);
                        stdout_acc.push('\n');
                    }
                }
                Stream::Err => {
                    if stderr_tail.len() >= 160 {
                        stderr_tail.pop_front();
                    }
                    stderr_tail.push_back(line.clone());
                }
            }
            on_line(stream, &line);
        }

        let wait_result = {
            let mut c = match child.lock() {
                Ok(c) => c,
                Err(_) => return Err("プロセスのロックに失敗しました".into()),
            };
            c.try_wait()
        };
        match wait_result {
            Ok(Some(st)) => {
                status = Some(st);
                break;
            }
            Ok(None) => {}
            Err(e) => return Err(format!("プロセスの待機に失敗しました: {e}")),
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Give the reader threads a moment to flush the final lines.
    let deadline = Instant::now() + Duration::from_millis(400);
    loop {
        match rx.try_recv() {
            Ok((Stream::Out, line)) => {
                if stdout_acc.len() < 8 * 1024 * 1024 {
                    stdout_acc.push_str(&line);
                    stdout_acc.push('\n');
                }
                on_line(Stream::Out, &line);
            }
            Ok((Stream::Err, line)) => {
                if stderr_tail.len() >= 160 {
                    stderr_tail.pop_front();
                }
                stderr_tail.push_back(line.clone());
                on_line(Stream::Err, &line);
            }
            Err(TryRecvError::Empty) => {
                if Instant::now() > deadline {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(TryRecvError::Disconnected) => break,
        }
    }

    job.detach_child();

    Ok(Outcome {
        status,
        stdout: stdout_acc,
        stderr: stderr_tail.into_iter().collect::<Vec<_>>().join("\n"),
        cancelled,
    })
}

pub fn parse_timecode(text: &str) -> Option<f64> {
    let text = text.trim();
    let parts: Vec<&str> = text.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let s: f64 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        2 => {
            let m: f64 = parts[0].parse().ok()?;
            let s: f64 = parts[1].parse().ok()?;
            Some(m * 60.0 + s)
        }
        _ => text.parse().ok(),
    }
}
