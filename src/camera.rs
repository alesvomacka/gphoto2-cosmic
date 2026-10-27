// SPDX-License-Identifier: MPL-2.0
//! Blocking gphoto2 boundary. Downloads share one process; no shell or camera deletion.
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Camera {
    pub model: String,
    pub port: String,
}
impl std::fmt::Display for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.model, self.port)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub status: String,
    pub imported: usize,
    pub skipped: usize,
    pub bytes_received: u64,
    pub current_bytes: u64,
    pub current_size: Option<u64>,
    pub started: Option<Instant>,
    pub finished: Option<Instant>,
    pub transfer_started: Option<Instant>,
}
impl Progress {
    pub fn elapsed(&self) -> Duration {
        self.started
            .map(|start| {
                self.finished
                    .unwrap_or_else(Instant::now)
                    .saturating_duration_since(start)
            })
            .unwrap_or_default()
    }

    pub fn fraction(&self) -> f32 {
        let current = self
            .current_size
            .filter(|size| *size > 0)
            .map(|size| self.current_bytes as f64 / size as f64)
            .unwrap_or(0.0);
        if self.total == 0 {
            return 0.0;
        }
        ((self.done as f64 + current) / self.total as f64).clamp(0.0, 1.0) as f32
    }

    fn transfer_elapsed(&self) -> Duration {
        self.transfer_started
            .map(|start| {
                self.finished
                    .unwrap_or_else(Instant::now)
                    .saturating_duration_since(start)
            })
            .unwrap_or_default()
    }

    pub fn average_speed(&self) -> Option<f64> {
        let elapsed = self.transfer_elapsed().as_secs_f64();
        (elapsed > 0.0 && (self.finished.is_some() || elapsed >= 0.5) && self.bytes_received > 0)
            .then(|| self.bytes_received as f64 / elapsed)
    }

    pub fn remaining(&self) -> Option<Duration> {
        if self.finished.is_some() || self.imported == 0 || self.done >= self.total {
            return None;
        }
        let seconds = self.transfer_elapsed().as_secs_f64() / self.imported as f64
            * (self.total - self.done) as f64;
        Duration::try_from_secs_f64(seconds).ok()
    }

    pub fn summary(&self) -> String {
        let mut text = format!(
            "Elapsed: {} · Received: {}",
            duration_text(self.elapsed()),
            bytes_text(self.bytes_received)
        );
        if let Some(speed) = self.average_speed() {
            text.push_str(&format!(" · Average: {}/s", bytes_text(speed as u64)));
        }
        if self.finished.is_none() {
            text.push_str(&match self.remaining() {
                Some(remaining) => format!(" · About {} remaining", duration_text(remaining)),
                None if self.total > 0 && self.done >= self.total => " · Finalizing…".into(),
                None => " · Estimating time remaining…".into(),
            });
        }
        text
    }

    pub fn current_file_summary(&self) -> Option<String> {
        self.current_size.map(|size| {
            format!(
                "Current file: {} / {}",
                bytes_text(self.current_bytes),
                bytes_text(size)
            )
        })
    }
}
fn bytes_text(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (divisor, unit) = if bytes < 1024 * 1024 {
        (1024.0, "KiB")
    } else if bytes < 1024 * 1024 * 1024 {
        (1024.0 * 1024.0, "MiB")
    } else {
        (1024.0 * 1024.0 * 1024.0, "GiB")
    };
    format!("{:.1} {unit}", bytes as f64 / divisor)
}
fn duration_text(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

/// Freeze timing on every return path, including cancellation and failures.
struct ProgressTimer(SharedProgress);
impl Drop for ProgressTimer {
    fn drop(&mut self) {
        self.0.lock().unwrap().finished = Some(Instant::now());
    }
}
pub type SharedProgress = Arc<Mutex<Progress>>;

fn command(camera: Option<&Camera>) -> Command {
    let mut cmd = Command::new("gphoto2");
    cmd.env("LC_ALL", "C").stdin(Stdio::null());
    if let Some(camera) = camera {
        cmd.args(["--camera", &camera.model, "--port", &camera.port]);
    }
    cmd
}
fn output(cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("Cannot run gphoto2: {e}. Install the gphoto2 package."))?;
    if !out.status.success() {
        return Err(format!(
            "gphoto2 failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    String::from_utf8(out.stdout)
        .map_err(|_| "Camera returned a non-UTF-8 file name; import stopped safely.".into())
}
pub fn detect() -> Result<Vec<Camera>, String> {
    Ok(parse_cameras(&output(command(None).arg("--auto-detect"))?))
}
fn parse_cameras(text: &str) -> Vec<Camera> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_end();
            let split = line.rfind(char::is_whitespace)?;
            let (model, port) = line.split_at(split);
            let port = port.trim();
            if !port.contains(':') || model.trim().is_empty() {
                return None;
            }
            Some(Camera {
                model: model.trim().into(),
                port: port.into(),
            })
        })
        .collect()
}
#[derive(Debug, PartialEq)]
struct RemoteFile {
    name: String,
    relative: PathBuf,
}
fn parse_files(text: &str) -> Result<Vec<RemoteFile>, String> {
    let mut files = Vec::new();
    for line in text.lines().filter(|s| !s.is_empty()) {
        let relative = line
            .strip_prefix('/')
            .ok_or("Unexpected gphoto2 file listing")?
            .trim_start_matches('/');
        let path = Path::new(relative);
        if path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Unsafe camera file path; import stopped.".into());
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("Missing camera file name")?;
        // Keep RAW, JPEG, video and sidecars: cameras may expose vendor-specific formats.
        files.push(RemoteFile {
            name: name.into(),
            relative: path.into(),
        });
    }
    Ok(files)
}
fn report(progress: &SharedProgress, done: usize, total: usize, status: String) {
    let mut progress = progress.lock().unwrap();
    progress.done = done;
    progress.total = total;
    progress.status = status;
    progress.current_bytes = 0;
    progress.current_size = None;
}

/// Owns and reaps the shared download process on success, cancellation, or any error.
struct DownloadProcess {
    child: Child,
    stderr: Option<JoinHandle<String>>,
    reaped: bool,
}
impl DownloadProcess {
    fn diagnostics(&mut self) -> String {
        self.stderr
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default()
    }

    fn stop(&mut self) -> String {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
        self.diagnostics()
    }

    fn finish(&mut self) -> Result<(), String> {
        let status = self
            .child
            .wait()
            .map_err(|e| format!("Wait for gphoto2: {e}"))?;
        self.reaped = true;
        let diagnostics = self.diagnostics();
        if !status.success() {
            return Err(format!("gphoto2 failed ({status}): {diagnostics}"));
        }
        Ok(())
    }
}
impl Drop for DownloadProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// gphoto2 --stdout-size emits decimal size + newline, then exactly that many bytes.
fn read_size(reader: &mut impl BufRead) -> Result<u64, String> {
    let mut header = Vec::new();
    reader
        .take(32)
        .read_until(b'\n', &mut header)
        .map_err(|e| format!("Read download size: {e}"))?;
    let digits = header
        .strip_suffix(b"\n")
        .ok_or("Missing or truncated gphoto2 size header")?;
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err("Invalid gphoto2 size header".into());
    }
    std::str::from_utf8(digits)
        .unwrap()
        .parse()
        .map_err(|_| "gphoto2 size is too large".into())
}

pub fn import(
    camera: Camera,
    destination: PathBuf,
    new_only: bool,
    preserve_folders: bool,
    progress: SharedProgress,
    cancel: Arc<AtomicBool>,
) -> Result<String, String> {
    *progress.lock().unwrap() = Progress {
        started: Some(Instant::now()),
        ..Default::default()
    };
    let _timer = ProgressTimer(progress.clone());
    let root = fs::canonicalize(&destination).map_err(|e| format!("Destination: {e}"))?;
    if !root.is_dir() {
        return Err("Destination must be a folder.".into());
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok("Cancelled. Imported 0; skipped 0.".into());
    }
    report(&progress, 0, 0, "Reading camera file list…".into());
    let files = parse_files(&output(command(Some(&camera)).args([
        "--quiet",
        "--recurse",
        "--list-files",
    ]))?)?;
    let total = files.len();
    let mut reserved: HashSet<String> = files.iter().map(|file| file.name.clone()).collect();
    let mut seen = HashSet::new();
    let mut targets = HashSet::new();
    let mut planned = Vec::new();
    let mut skipped = 0;
    for file in files {
        if cancel.load(Ordering::Relaxed) {
            return Ok(format!("Cancelled. Imported 0; skipped {skipped}."));
        }
        let mut parent = root.clone();
        let folders = if preserve_folders {
            file.relative.parent().unwrap()
        } else {
            Path::new("")
        };
        for component in folders.components() {
            parent.push(component);
            match fs::create_dir(&parent) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("Create folder: {e}")),
            }
            let metadata = fs::symlink_metadata(&parent).map_err(|e| e.to_string())?;
            if !metadata.file_type().is_dir() {
                return Err("Destination subfolder is a symlink or is not a directory.".into());
            }
        }
        let mut name = file.name.clone();
        if !preserve_folders && !seen.insert(file.name.clone()) {
            let mut n = 1;
            loop {
                name = format!("{n}-{}", file.name);
                if reserved.insert(name.clone()) {
                    break;
                }
                n += 1;
            }
        }
        let mut target = parent.join(&name);
        if target.try_exists().map_err(|e| e.to_string())? || targets.contains(&target) {
            if new_only {
                skipped += 1;
                progress.lock().unwrap().skipped = skipped;
                report(&progress, skipped, total, format!("Skipped {}", file.name));
                continue;
            }
            let mut n = 1;
            while target.try_exists().map_err(|e| e.to_string())? || targets.contains(&target) {
                target = parent.join(format!("{n}-{name}"));
                n += 1;
            }
        }
        targets.insert(target.clone());
        planned.push((file, target));
    }
    if planned.is_empty() {
        return Ok(format!(
            "Finished. Imported 0; skipped {skipped}. Camera files were kept."
        ));
    }

    // Each --get-file runs sequentially against the same initialized camera/session.
    let mut cmd = command(Some(&camera));
    cmd.args(["--quiet", "--folder", "/", "--no-recurse", "--stdout-size"]);
    for (file, _) in &planned {
        cmd.arg("--get-file").arg(&file.relative);
    }
    progress.lock().unwrap().transfer_started = Some(Instant::now());
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Cannot start gphoto2 download: {e}"))?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    // Always drain stderr, including after its bounded diagnostic tail is full.
    let errors = thread::spawn(move || {
        let mut tail = Vec::new();
        let mut buffer = [0; 4096];
        while let Ok(count) = stderr.read(&mut buffer) {
            if count == 0 {
                break;
            }
            tail.extend_from_slice(&buffer[..count]);
            if tail.len() > 16384 {
                tail.drain(..tail.len() - 16384);
            }
        }
        String::from_utf8_lossy(&tail).trim().to_owned()
    });
    let mut process = DownloadProcess {
        child,
        stderr: Some(errors),
        reaped: false,
    };
    let mut reader = BufReader::new(stdout);
    report(
        &progress,
        skipped,
        total,
        "Starting shared camera download…".into(),
    );
    let mut buffer = vec![0; 256 * 1024];
    for (imported, (file, target)) in planned.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            process.stop();
            return Ok(format!(
                "Cancelled. Imported {imported}; skipped {skipped}."
            ));
        }
        report(
            &progress,
            skipped + imported,
            total,
            format!("Importing {}", file.name),
        );
        let temporary = target.parent().unwrap().join(format!(
            ".camera-import-{}-{imported}.part",
            std::process::id()
        ));
        let mut local = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| format!("Create temporary file: {e}"))?;
        let result = (|| {
            let size = read_size(&mut reader)?;
            progress.lock().unwrap().current_size = Some(size);
            let mut copied = 0;
            while copied < size {
                let chunk = (size - copied).min(buffer.len() as u64) as usize;
                let count = reader
                    .read(&mut buffer[..chunk])
                    .map_err(|e| format!("Download {}: {e}", file.name))?;
                if count == 0 {
                    return Err(format!(
                        "Truncated download of {}: {copied}/{size} bytes",
                        file.name
                    ));
                }
                local
                    .write_all(&buffer[..count])
                    .map_err(|e| format!("Write {}: {e}", file.name))?;
                copied += count as u64;
                let mut progress = progress.lock().unwrap();
                progress.current_bytes = copied;
                progress.bytes_received += count as u64;
            }
            // Atomic visibility without an fsync stall between every pair of photos.
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                &temporary,
                rustix::fs::CWD,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(|e| format!("Save {}: {e}", target.display()))?;
            Ok(())
        })();
        drop(local);
        let _ = fs::remove_file(&temporary);
        if let Err(error) = result {
            let diagnostics = process.stop();
            return Err(if diagnostics.is_empty() {
                error
            } else {
                format!("{error}: {diagnostics}")
            });
        }
        progress.lock().unwrap().imported = imported + 1;
        report(
            &progress,
            skipped + imported + 1,
            total,
            format!("Imported {}", file.name),
        );
    }
    let mut extra = [0];
    if reader
        .read(&mut extra)
        .map_err(|e| format!("Read gphoto2 output: {e}"))?
        != 0
    {
        let diagnostics = process.stop();
        return Err(format!("Unexpected extra gphoto2 output: {diagnostics}"));
    }
    process.finish()?;
    Ok(format!(
        "Finished. Imported {}; skipped {skipped}. Camera files were kept.",
        total - skipped
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    #[test]
    fn progress_metrics_exclude_setup_and_skipped_files() {
        let start = Instant::now();
        let progress = Progress {
            started: Some(start),
            transfer_started: Some(start + Duration::from_secs(10)),
            finished: Some(start + Duration::from_secs(14)),
            imported: 2,
            skipped: 3,
            done: 5,
            total: 5,
            bytes_received: 8 * 1024 * 1024,
            ..Default::default()
        };
        assert_eq!(progress.elapsed(), Duration::from_secs(14));
        assert_eq!(progress.average_speed(), Some(2.0 * 1024.0 * 1024.0));
        assert_eq!(progress.fraction(), 1.0);
        assert_eq!(progress.remaining(), None);
        assert!(progress.summary().contains("2.0 MiB/s"));
        assert!(!progress.summary().contains("Estimating"));
    }

    #[test]
    fn progress_handles_partial_files_and_unknown_eta() {
        let mut progress = Progress {
            done: 2,
            total: 4,
            current_bytes: 50,
            current_size: Some(100),
            ..Default::default()
        };
        assert_eq!(progress.fraction(), 0.625);
        assert_eq!(progress.remaining(), None);
        assert_eq!(progress.average_speed(), None);
        progress.current_size = Some(0);
        assert_eq!(progress.fraction(), 0.5);
        progress.total = 0;
        assert_eq!(progress.fraction(), 0.0);
    }

    #[test]
    fn timing_freezes_on_error_or_cancellation() {
        let progress = SharedProgress::default();
        {
            let _timer = ProgressTimer(progress.clone());
        }
        assert!(progress.lock().unwrap().finished.is_some());
        assert_eq!(duration_text(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn size_headers_reject_invalid_or_unbounded_input() {
        for bytes in [
            b"".as_slice(),
            b"-1\n",
            b"2x\n",
            b"42",
            b"\n",
            b"18446744073709551616\n",
            b"999999999999999999999999999999999999999\n",
        ] {
            assert!(read_size(&mut io::Cursor::new(bytes)).is_err());
        }
        assert_eq!(read_size(&mut io::Cursor::new(b"0\n")).unwrap(), 0);
    }

    #[test]
    fn detects_multiple_models_with_spaces() {
        let cameras = parse_cameras(
            "Model                          Port\n----------------------------------------------------------\nNikon Z fc                     usb:001,002    \nCanon EOS R                    usb:001,003\n",
        );
        assert_eq!(cameras.len(), 2);
        assert_eq!(cameras[0].model, "Nikon Z fc");
        assert_eq!(cameras[0].port, "usb:001,002");
    }
    #[test]
    fn paths_keep_spaces_and_folders() {
        let files = parse_files("/store/DCIM/100NIKON/a b.NEF\n/store/DCIM/100NIKON/c.JPG\n/store/DCIM/101NIKON/a.JPG\n").unwrap();
        assert_eq!(files[0].name, "a b.NEF");
        assert_eq!(
            parse_files("//root.JPG\n").unwrap()[0].relative,
            Path::new("root.JPG")
        );
        assert_eq!(files[1].relative, Path::new("store/DCIM/100NIKON/c.JPG"));
        assert_eq!(files[2].relative, Path::new("store/DCIM/101NIKON/a.JPG"));
    }
    #[test]
    fn rejects_traversal_and_unexpected_output() {
        for path in [
            "/../secret.JPG",
            "/DCIM/../../secret.JPG",
            "unexpected output",
        ] {
            assert!(parse_files(path).is_err());
        }
    }
}
