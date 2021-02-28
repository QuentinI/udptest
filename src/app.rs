use std::{path::Path, sync::mpsc};

use eframe::{egui, epi};
use log::{error, info, warn};
use rusqlite::Connection;

use crate::{record::Record, udp::Receiver, udp::Sender};

#[derive(PartialEq, Eq)]
/// Represents app modes
pub enum Mode {
    Send,
    Listen,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Send
    }
}

/// A type for control messages, sent by UI thread to
/// Worker thread.
pub enum ControlMessage {
    /// Signals the thread to gracefully shutdown.
    Stop,
}

/// A type for messages worker threads can send to UI thread
/// to indicate completion or provide log data
pub enum StatusMessage {
    /// Worker thread exited successfully.
    Success,
    /// Worker thread failed.
    Failure(String),
    /// A non-fatal error occured in worker thread, and
    /// it wats us to notify the user about it.
    Warning(String),
    /// Worker thread is still running, but wants us to display
    /// a message to the user.
    Info(String),
}

struct Task {
    control: mpsc::Sender<ControlMessage>,
    status: mpsc::Receiver<StatusMessage>,
}

/// This struct represents application state.
pub struct App {
    /// Wheter we want to render with DPI value of 2.
    hdpi: bool,
    /// Selected mode.
    mode: Mode,
    /// Address we will bind to for transmission or receving.
    bind_addr: String,
    /// Address we transmit to.
    tx_addr: String,
    /// Path to database to read records from.
    db_file: String,
    /// Wraps control and status channels for currently running worker thread.
    task: Option<Task>,
    /// Whether previous worker finished successfully.
    status: Option<bool>,
    /// Log displayed to user.
    log: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            hdpi: true,
            mode: Mode::default(),
            bind_addr: "0.0.0.0:8142".to_owned(),
            tx_addr: "".to_owned(),
            db_file: "test/test.sqlite".to_owned(),
            task: None,
            status: None,
            log: String::new(),
        }
    }
}

impl App {
    /// Controls UI and worker for [Mode::Send] mode.
    fn sender(&mut self, ui: &mut egui::Ui) {
        ui.set_enabled(self.task.is_none());

        ui.label("Bind to address");
        ui.text_edit_singleline(&mut self.bind_addr)
            .on_hover_text("Interface and port to bind to");
        ui.label("Send to address");
        ui.text_edit_singleline(&mut self.tx_addr)
            .on_hover_text("Address and port to send to");
        ui.label("Read data from");
        ui.text_edit_singleline(&mut self.db_file)
            .on_hover_text("sqlite file to read from");

        if self.task.is_some() {
            ui.label("Running...");
        } else {
            if ui.button("Run").clicked() {
                let (control_sender, control_receiver) = std::sync::mpsc::channel();
                let (status_sender, status_receiver) = std::sync::mpsc::channel();
                self.task = Some(Task {
                    control: control_sender,
                    status: status_receiver,
                });

                let addr = self.bind_addr.clone();
                let path_str = self.db_file.clone();
                let dest = self.tx_addr.clone();

                std::thread::spawn(move || -> Result<(), ()> {
                    // Although we don't use it, take in case UI thread
                    // tries to send us messages
                    let _receiver = control_receiver;

                    status_sender
                        .send(StatusMessage::Info("Sending data...".into()))
                        .unwrap();

                    let mut udp_sender = Sender::new(addr).map_err(|e| {
                        status_sender
                            .send(StatusMessage::Failure(format!(
                                "Couldn't bind to buffer: {}",
                                e
                            )))
                            .unwrap();
                    })?;

                    let path = Path::new(&path_str);
                    if !path.is_file() {
                        status_sender
                            .send(StatusMessage::Failure(format!(
                                "No such file: {}",
                                path_str
                            )))
                            .unwrap();
                        return Err(());
                    }

                    let conn = Connection::open(path).map_err(|e| {
                        status_sender
                            .send(StatusMessage::Failure(format!("Couldn't open file: {}", e)))
                            .unwrap();
                    })?;

                    let data = Record::load(conn).map_err(|e| {
                        status_sender
                            .send(StatusMessage::Failure(format!(
                                "Couldn't load records from DB: {}",
                                e
                            )))
                            .unwrap();
                    })?;

                    udp_sender.send(data.iter(), dest).map_err(|e| {
                        status_sender
                            .send(StatusMessage::Failure(format!("Error sending data: {}", e)))
                            .unwrap();
                    })?;

                    status_sender
                        .send(StatusMessage::Info("Done!".into()))
                        .unwrap();
                    status_sender.send(StatusMessage::Success).unwrap();
                    Ok(())
                });
            }
        }
    }

    /// Controls UI and worker for [Mode::Listen] mode.
    fn listener(&mut self, ui: &mut egui::Ui) {
        ui.label("Listen on address");

        ui.wrap(|ui| {
            ui.set_enabled(self.task.is_none());
            ui.text_edit_singleline(&mut self.bind_addr);
        });

        if let Some(ref mut task) = self.task {
            if ui.button("Stop").clicked() {
                task.control.send(ControlMessage::Stop).unwrap();
            }
        } else {
            if ui.button("Listen").clicked() {
                let (control_sender, control_receiver) = std::sync::mpsc::channel();
                let (status_sender, status_receiver) = std::sync::mpsc::channel();

                self.task = Some(Task {
                    control: control_sender,
                    status: status_receiver,
                });

                let addr = self.bind_addr.clone();

                std::thread::spawn(move || -> Result<(), ()> {
                    let mut udp_receiver: Receiver<Record> = Receiver::new(&addr).map_err(|e| {
                        status_sender
                            .send(StatusMessage::Failure(format!(
                                "Couldn't bind to address: {}",
                                e
                            )))
                            .unwrap()
                    })?;

                    status_sender
                        .send(StatusMessage::Info(format!("Listening on {}...", &addr)))
                        .unwrap();

                    loop {
                        let res = udp_receiver.next().expect("Never returns None");
                        match res {
                            Ok(record) => {
                                let msg = format!("Got record [{} : {}]", record.id, record.data);
                                status_sender.send(StatusMessage::Info(msg)).unwrap();
                            }
                            Err(crate::udp::Error::ParseError(_)) => {
                                status_sender
                                    .send(StatusMessage::Warning("Got corrupted packet".into()))
                                    .unwrap();
                            }
                            Err(crate::udp::Error::Io(e)) => {
                                if e.kind() != std::io::ErrorKind::TimedOut
                                    && e.kind() != std::io::ErrorKind::WouldBlock
                                {
                                    let msg = format!(
                                        "Error while reading from socket: {}",
                                        e.to_string(),
                                    );
                                    status_sender.send(StatusMessage::Warning(msg)).unwrap();
                                }
                            }
                        }
                        if let Ok(ControlMessage::Stop) = control_receiver.try_recv() {
                            status_sender
                                .send(StatusMessage::Info("Stopped".into()))
                                .unwrap();
                            break;
                        }
                    }

                    status_sender.send(StatusMessage::Success).unwrap();

                    Ok(())
                });
            }
        }
    }
}

impl epi::App for App {
    fn name(&self) -> &str {
        "UDP Test app"
    }

    fn update(&mut self, ctx: &egui::CtxRef, _frame: &mut epi::Frame<'_>) {
        if self.hdpi {
            ctx.set_pixels_per_point(2.0);
        } else {
            ctx.set_pixels_per_point(1.0);
        }

        egui::TopPanel::top("wrap_app_top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                let style: egui::Style = (*ui.ctx().style()).clone();
                let new_visuals = style.visuals.light_dark_small_toggle_button(ui);
                if let Some(visuals) = new_visuals {
                    ui.ctx().set_visuals(visuals);
                }
                ui.checkbox(&mut self.hdpi, "HiDPI");

                ui.wrap(|ui| {
                    ui.set_enabled(self.task.is_none());
                    ui.selectable_value(&mut self.mode, Mode::Send, "Send");
                    ui.selectable_value(&mut self.mode, Mode::Listen, "Listen");
                });
            });
        });

        if let Some(ref task) = self.task {
            if let Ok(message) = task.status.try_recv() {
                match message {
                    StatusMessage::Success => {
                        self.status = Some(true);
                        self.task = None;
                    }
                    StatusMessage::Failure(status) => {
                        let msg = format!("{}\n", status);
                        self.log.push_str(&msg);
                        error!("{}", status);
                        self.status = Some(false);
                        self.task = None;
                    }
                    StatusMessage::Warning(status) => {
                        let msg = format!("{}\n", status);
                        self.log.push_str(&msg);
                        warn!("{}", status);
                    }
                    StatusMessage::Info(status) => {
                        let msg = format!("{}\n", status);
                        self.log.push_str(&msg);
                        info!("{}", status);
                    }
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.vertical(|ui| {
                    let mut stroke = ui.style().visuals.window_stroke();
                    stroke.color = if self.task.is_some() {
                        egui::Color32::BLUE.linear_multiply(0.2)
                    } else {
                        match self.status {
                            Some(true) => egui::Color32::GREEN.linear_multiply(0.2),
                            Some(false) => egui::Color32::RED.linear_multiply(0.2),
                            None => stroke.color,
                        }
                    };
                    egui::Frame::group(ui.style())
                        .stroke(stroke)
                        .show(ui, |ui| match self.mode {
                            Mode::Listen => self.listener(ui),
                            Mode::Send => self.sender(ui),
                        })
                });
                egui::ScrollArea::auto_sized().show(ui, |ui| {
                    ui.set_enabled(false);
                    ui.add(egui::TextEdit::multiline(&mut self.log).frame(false));
                });
            });
        });
    }
}

pub fn run() -> ! {
    env_logger::init();

    let app = App::default();
    eframe::run_native(Box::new(app));
}
