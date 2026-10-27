// SPDX-License-Identifier: MPL-2.0
// Panel lifecycle adapted from pop-os/cosmic-applet-template (see NOTICE).
use crate::camera::{self, Camera, SharedProgress};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::{
    iced::{self, Limits, Subscription, window::Id},
    prelude::*,
    widget,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub struct App {
    core: cosmic::Core,
    popup: Option<Id>,
    standalone: bool,
    cameras: Vec<Camera>,
    labels: Vec<String>,
    selected: Option<usize>,
    destination: String,
    new_only: bool,
    dated: bool,
    preserve_folders: bool,
    busy: bool,
    importing: bool,
    choosing: bool,
    status: String,
    progress: SharedProgress,
    cancel: Arc<AtomicBool>,
}
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    Closed(Id),
    Refresh,
    Detected(Result<Vec<Camera>, String>),
    Select(usize),
    Destination(String),
    Choose,
    Chosen(Result<Option<PathBuf>, String>),
    NewOnly(bool),
    Dated(bool),
    PreserveFolders(bool),
    Import,
    Finished(Result<String, String>),
    Tick,
    Cancel,
}
impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = bool;
    type Message = Message;
    const APP_ID: &'static str = "io.github.alesvomacka.CameraImport";
    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }
    fn init(core: cosmic::Core, standalone: bool) -> (Self, Task<cosmic::Action<Message>>) {
        let mut app = Self {
            core,
            popup: None,
            standalone,
            cameras: vec![],
            labels: vec![],
            selected: None,
            destination: std::env::var("HOME")
                .map(|h| format!("{h}/Pictures"))
                .unwrap_or_default(),
            new_only: true,
            dated: false,
            preserve_folders: false,
            busy: false,
            importing: false,
            choosing: false,
            status: String::new(),
            progress: Arc::new(Mutex::new(Default::default())),
            cancel: Arc::new(AtomicBool::new(false)),
        };
        if standalone {
            app.set_header_title("Camera Import Applet for COSMIC".into());
        }
        let task = app.update(Message::Refresh);
        (app, task)
    }
    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::Closed(id))
    }
    fn view(&self) -> Element<'_, Message> {
        if self.standalone {
            return self.content();
        }
        self.core
            .applet
            .icon_button("io.github.alesvomacka.CameraImport-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }
    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        self.core.applet.popup_container(self.content()).into()
    }
    fn subscription(&self) -> Subscription<Message> {
        if self.busy {
            iced::time::every(Duration::from_millis(200)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        }
    }
    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    return destroy_popup(id);
                }
                let id = Id::unique();
                self.popup = Some(id);
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    id,
                    None,
                    None,
                    None,
                );
                settings.positioner.size_limits = Limits::NONE
                    .min_width(360.)
                    .max_width(440.)
                    .min_height(300.)
                    .max_height(900.);
                return get_popup(settings);
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                } else if self.standalone {
                    self.cancel.store(true, Ordering::Relaxed);
                    return cosmic::iced::exit();
                }
            }
            Message::Refresh if !self.busy => {
                self.busy = true;
                self.status = "Detecting cameras…".into();
                *self.progress.lock().unwrap() = Default::default();
                return cosmic::task::future(async {
                    Message::Detected(
                        tokio::task::spawn_blocking(camera::detect)
                            .await
                            .unwrap_or_else(|e| Err(e.to_string())),
                    )
                });
            }
            Message::Detected(result) => {
                self.busy = false;
                match result {
                    Ok(cameras) => {
                        let previous = self.selected.and_then(|i| self.cameras.get(i)).cloned();
                        self.selected = previous
                            .and_then(|old| cameras.iter().position(|c| *c == old))
                            .or(if cameras.is_empty() { None } else { Some(0) });
                        self.labels = cameras.iter().map(ToString::to_string).collect();
                        self.cameras = cameras;
                        self.status = if self.cameras.is_empty() {
                            "No cameras found. Connect and turn on a camera, then Refresh.".into()
                        } else {
                            "Ready to import.".into()
                        };
                    }
                    Err(error) => {
                        self.cameras.clear();
                        self.labels.clear();
                        self.selected = None;
                        self.status = error;
                    }
                }
            }
            Message::Select(index) if !self.busy => self.selected = Some(index),
            Message::Destination(path) if !self.busy => self.destination = path,
            Message::NewOnly(value) if !self.busy => self.new_only = value,
            Message::Dated(value) if !self.busy => self.dated = value,
            Message::PreserveFolders(value) if !self.busy => self.preserve_folders = value,
            Message::Choose if !self.busy && !self.choosing => {
                self.choosing = true;
                return cosmic::task::future(async {
                    use cosmic::dialog::file_chooser::{self, open::Dialog};
                    Message::Chosen(
                        match Dialog::new()
                            .title("Choose import destination")
                            .open_folder()
                            .await
                        {
                            Ok(response) => response
                                .url()
                                .to_file_path()
                                .map(Some)
                                .map_err(|_| "Choose a local folder.".into()),
                            Err(file_chooser::Error::Cancelled) => Ok(None),
                            Err(e) => Err(format!("Folder chooser: {e}")),
                        },
                    )
                });
            }
            Message::Chosen(result) => {
                self.choosing = false;
                match result {
                    Ok(Some(path)) => self.destination = path.to_string_lossy().into(),
                    Ok(None) => {}
                    Err(error) => self.status = error,
                }
            }
            Message::Import if !self.busy && !self.choosing => {
                let Some(camera) = self.selected.and_then(|i| self.cameras.get(i)).cloned() else {
                    return Task::none();
                };
                let destination = PathBuf::from(&self.destination);
                if !destination.is_absolute() {
                    self.status = "Choose an absolute destination folder.".into();
                    return Task::none();
                }
                self.busy = true;
                self.cancel.store(false, Ordering::Relaxed);
                *self.progress.lock().unwrap() = Default::default();
                self.status = "Starting import…".into();
                self.importing = true;
                let (new_only, dated, preserve_folders, progress, cancel) = (
                    self.new_only,
                    self.dated,
                    self.preserve_folders,
                    self.progress.clone(),
                    self.cancel.clone(),
                );
                return cosmic::task::future(async move {
                    Message::Finished(
                        tokio::task::spawn_blocking(move || {
                            let destination = if dated {
                                // Local export date, supplied by the standard coreutils date command.
                                let date = std::process::Command::new("date")
                                    .arg("+%F")
                                    .output()
                                    .map_err(|e| e.to_string())?;
                                if !date.status.success() {
                                    return Err("Cannot determine export date.".into());
                                }
                                let date = String::from_utf8_lossy(&date.stdout).trim().to_owned();
                                if date.len() != 10
                                    || !date.chars().all(|c| c.is_ascii_digit() || c == '-')
                                {
                                    return Err("Invalid export date.".into());
                                }
                                destination.join(date)
                            } else {
                                destination
                            };
                            std::fs::create_dir_all(&destination)
                                .map_err(|e| format!("Destination: {e}"))?;
                            camera::import(
                                camera,
                                destination,
                                new_only,
                                preserve_folders,
                                progress,
                                cancel,
                            )
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string())),
                    )
                });
            }
            Message::Finished(result) => {
                self.busy = false;
                self.importing = false;
                self.status = result.unwrap_or_else(|e| format!("Import stopped: {e}"));
            }
            Message::Cancel if self.importing => {
                self.cancel.store(true, Ordering::Relaxed);
                self.status = "Cancelling after the current camera operation…".into();
            }
            Message::Tick => {}
            _ => {}
        }
        Task::none()
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        if self.standalone {
            None
        } else {
            Some(cosmic::applet::style())
        }
    }
}
impl App {
    fn content(&self) -> Element<'_, Message> {
        let progress = self.progress.lock().unwrap().clone();
        let mut refresh = widget::button::standard("Refresh cameras");
        if !self.busy {
            refresh = refresh.on_press(Message::Refresh);
        }
        let mut choose = widget::button::standard(if self.choosing {
            "Choosing…"
        } else {
            "Choose folder…"
        });
        if !self.busy && !self.choosing {
            choose = choose.on_press(Message::Choose);
        }
        let mut import = widget::button::suggested("Import");
        if !self.busy && !self.choosing && self.selected.is_some() && !self.destination.is_empty() {
            import = import.on_press(Message::Import);
        }
        let mut content = widget::column::with_capacity(16)
            .spacing(12)
            .padding(16)
            .push(widget::text::title3("Camera Import Applet for COSMIC"))
            .push(refresh);
        if !self.cameras.is_empty() {
            content = content.push(widget::dropdown(
                &self.labels,
                self.selected,
                Message::Select,
            ));
        }
        content = content
            .push(widget::text("Destination"))
            .push(
                widget::text_input("/home/you/Pictures", &self.destination)
                    .on_input(Message::Destination),
            )
            .push(choose)
            .push(
                widget::checkbox(self.new_only)
                    .label("Import new only (skip existing paths)")
                    .on_toggle(Message::NewOnly),
            )
            .push(
                widget::checkbox(self.dated)
                    .label("Subfolder by export date (YYYY-MM-DD)")
                    .on_toggle(Message::Dated),
            )
            .push(
                widget::checkbox(self.preserve_folders)
                    .label("Preserve camera folder structure")
                    .on_toggle(Message::PreserveFolders),
            )
            .push(widget::text("Originals stay on the camera."))
            .push(widget::text(&self.status));
        if progress.started.is_some() {
            if progress.total > 0 {
                content = content
                    .push(widget::determinate_linear(progress.fraction()))
                    .push(widget::text(format!(
                        "{} / {} files · {} imported · {} skipped",
                        progress.done, progress.total, progress.imported, progress.skipped
                    )));
            }
            if self.importing && !progress.status.is_empty() {
                content = content.push(widget::text(progress.status.clone()));
                if let Some(current) = progress.current_file_summary() {
                    content = content.push(widget::text(current));
                }
            }
            content = content.push(widget::text(progress.summary()));
        }
        content = content.push(import);
        if self.importing {
            content =
                content.push(widget::button::standard("Cancel import").on_press(Message::Cancel));
        }
        widget::scrollable(content)
            .height(iced::Length::Shrink)
            .into()
    }
}
