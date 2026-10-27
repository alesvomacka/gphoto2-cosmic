// SPDX-License-Identifier: MPL-2.0
//! Process-isolated driver used by tests/test_backend.py; not installed.
#[path = "../../src/camera.rs"]
mod camera;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args[1] == "detect" {
        println!("{:?}", camera::detect().unwrap());
        return;
    }
    let progress: camera::SharedProgress = Arc::new(Mutex::new(Default::default()));
    let cancel = Arc::new(AtomicBool::new(args[1] == "cancel"));
    if let Ok(delay) = std::env::var("TEST_CANCEL_AFTER_MS") {
        let cancel = cancel.clone();
        let progress = progress.clone();
        std::thread::spawn(move || {
            while !progress.lock().unwrap().status.starts_with("Importing ") {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(delay.parse().unwrap()));
            cancel.store(true, Ordering::Relaxed);
        });
    }
    let observing = Arc::new(AtomicBool::new(true));
    let observer = std::env::var_os("TEST_OBSERVE_PROGRESS").map(|_| {
        let observing = observing.clone();
        let progress = progress.clone();
        std::thread::spawn(move || {
            let mut observed_partial = false;
            while observing.load(Ordering::Relaxed) {
                let progress = progress.lock().unwrap();
                observed_partial |= progress.current_bytes > 0
                    && progress
                        .current_size
                        .is_some_and(|size| progress.current_bytes < size);
                drop(progress);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            observed_partial
        })
    });
    let result = camera::import(
        camera::Camera {
            model: std::env::var("TEST_CAMERA_MODEL").unwrap_or("Nikon Z fc".into()),
            port: std::env::var("TEST_CAMERA_PORT").unwrap_or("usb:001,002".into()),
        },
        PathBuf::from(&args[2]),
        args[1] != "all",
        std::env::var_os("TEST_PRESERVE_FOLDERS").is_some(),
        progress.clone(),
        cancel,
    );
    observing.store(false, Ordering::Relaxed);
    if let Some(observer) = observer {
        eprintln!("Observed partial progress: {}", observer.join().unwrap());
    }
    let progress = progress.lock().unwrap();
    eprintln!(
        "Progress: {}/{} {}",
        progress.done, progress.total, progress.status
    );
    eprintln!(
        "{} · fraction={} · current={:?}",
        progress.summary(),
        progress.fraction(),
        progress.current_file_summary()
    );
    eprintln!(
        "Metrics: imported={} skipped={} bytes={} frozen={}",
        progress.imported,
        progress.skipped,
        progress.bytes_received,
        progress.finished.is_some()
    );
    match result {
        Ok(status) => println!("{status}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
