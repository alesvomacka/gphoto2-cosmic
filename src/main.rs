// SPDX-License-Identifier: MPL-2.0
mod app;
mod camera;

fn main() -> cosmic::iced::Result {
    if std::env::args().any(|arg| arg == "--window") {
        cosmic::app::run::<app::App>(
            cosmic::app::Settings::default().size(cosmic::iced::Size::new(460.0, 650.0)),
            true,
        )
    } else {
        cosmic::applet::run::<app::App>(false)
    }
}
