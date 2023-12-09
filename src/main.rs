#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::Ui;
use grammers_client::{client::chats::ParticipantIter, types::LoginToken, Client, SignInError};
use tokio::runtime;

use telegram_group_scraper::{get_client, get_participants};

#[derive(Default)]
struct MyApp {
    picked_path: Option<String>,
    chat_name: String,
    phone: String,
    otp: String,
    auth_token: Option<LoginToken>,
    signed_in: bool,
    result: Option<(ParticipantIter, usize)>,
    tg_client: Option<Client>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.signed_in {
                self.main_flow(ui);
            } else {
                self.telegram_auth_flow(ui);
            }
        });
    }
}

impl MyApp {
    fn telegram_auth_flow(&mut self, ui: &mut Ui) {
        if self.tg_client.is_none() {
            if ui.button("Connect").clicked() {
                ui.label("Connecting to telegram...");
                match telegram_group_scraper::get_client() {
                    Ok(client) => {
                        self.tg_client = Some(client);
                    }
                    Err(msg) => {
                        ui.label("Connection failed! ");
                        ui.monospace(msg.to_string());
                    }
                };
            }
        }
        // request OTP
        ui.vertical(|ui| {
            if self.tg_client.is_none() {
                return;
            }
            ui.label("Your phone number (in international format)");
            ui.text_edit_singleline(&mut self.phone);
            if ui.button("Start authentication").clicked() {
                println!("Start auth...");
                ui.monospace("Requesting OTP to telegram...");
                match tokio::runtime::Runtime::new().unwrap().block_on(
                    self.tg_client
                        .as_ref()
                        .unwrap()
                        .request_login_code(&self.phone.trim()),
                ) {
                    Ok(token) => {
                        self.auth_token = Some(token);
                    }
                    Err(msg) => {
                        ui.label("Got error: ");
                        ui.monospace(msg.to_string());
                    }
                }
            }
        });
        // use OTP to authenticate
        ui.vertical(|ui| {
            if self.auth_token.is_none() {
                return;
            }
            ui.label("Enter the OTP code you received");
            ui.text_edit_singleline(&mut self.otp);
            if ui.button("Submit").clicked() {
                ui.monospace("Validating OTP with telegram...");
                match tokio::runtime::Runtime::new().unwrap().block_on(
                    self.tg_client
                        .as_ref()
                        .unwrap()
                        .sign_in(&self.auth_token.as_ref().unwrap(), &self.otp),
                ) {
                    Ok(_) => {
                        self.signed_in = true;
                    }
                    Err(SignInError::PasswordRequired(password_token)) => {
                        ui.label("PASSWORD RICHIESTA??? CONTATTA UMB");
                    }
                    Err(msg) => {
                        ui.label("Got error: ");
                        ui.monospace(msg.to_string());
                    }
                }
            }
        });
    }

    fn main_flow(&mut self, ui: &mut Ui) {
        ui.label("Telegram chat names (one per line):");
        ui.text_edit_multiline(&mut self.chat_name);

        ui.label("Destination folder:");
        if ui.button("Open file…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                self.picked_path = Some(path.display().to_string());
            }
        }

        if let Some(picked_path) = &self.picked_path {
            ui.horizontal(|ui| {
                ui.label("Picked folder:");
                ui.monospace(picked_path);
                if ui.button("Go").clicked() {
                    let lines: Vec<_> = self.chat_name.lines().collect();
                    for (i, chat_name) in lines.iter().enumerate() {
                        ui.monospace(format!("({}/{}) Doing {} ...", i, lines.len(), chat_name));
                        let maybe_participants = get_participants(
                            self.tg_client.clone().unwrap(),
                            chat_name.to_string(),
                        );
                        match maybe_participants {
                            Ok(result) => {
                                self.result = Some(result);
                            }
                            Err(x) => {}
                        }
                    }
                }
            });
            if self.result.is_some() {
                ui.label(format!(
                    "There are {} participants in the group",
                    self.result.as_ref().unwrap().1
                ));
            }
        }
    }
}

fn main() -> () {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 240.0]) // wide enough for the drag-drop overlay text
            .with_drag_and_drop(true),
        ..Default::default()
    };
    // let app = Box::<MyApp>::default();
    let app = MyApp::default();
    eframe::run_native(
        "Pigna telegram scraper",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .expect("GUI crashed");
    // println!("{}", app.picked_path.clone().unwrap());
}
