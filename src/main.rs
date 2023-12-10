#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::sync::Arc;

use eframe::egui;
use egui::Ui;
use grammers_client::{client::chats::ParticipantIter, types::LoginToken, Client};

use telegram_group_scraper::{
    get_client, get_participants, Task, TaskResult, TaskSpawner, TaskType,
};

// #[derive(Default)]
// struct AuthenticationData {
//     phone: String,
//     otp: String,
//     auth_token: Option<LoginToken>,
//     signed_in: bool,
// }

#[derive(Default)]
enum TelegramState {
    #[default]
    // InitClient,
    NeedOTP, // phone number
    ValidateOTP(Arc<LoginToken>),
    LoggedIn,
}

struct TelegramGroupInfoApp {
    picked_path: Option<String>,
    chat_name: String,
    phone_number: String,
    otp_field: String,
    result: Vec<(ParticipantIter, usize)>,
    telegram_state: TelegramState,
    spawner: TaskSpawner,

    telegram_tx: tokio::sync::mpsc::Sender<TaskResult>,
    telegram_rx: tokio::sync::mpsc::Receiver<TaskResult>,
}

impl eframe::App for TelegramGroupInfoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_async_updates();
        egui::CentralPanel::default().show(ctx, |ui| match &self.telegram_state {
            // TelegramState::InitClient => {
            //     if ui.button("Connect to telegram").clicked() {
            //         self.client = get_client().unwrap();
            //         self.telegram_state = TelegramState::NeedOTP
            //     }
            // }
            TelegramState::NeedOTP => {
                ui.label("Phone number (with international prefix)");
                ui.text_edit_singleline(&mut self.phone_number);
                if ui.button("Request OTP").clicked() {
                    println!("OTP requested.");
                    self.spawner.spawn_task(Task {
                        task_type: TaskType::RequestOTP(self.phone_number.clone()),
                        // client: self.client.as_ref().unwrap().clone(),
                        result: self.telegram_tx.clone(),
                    })
                }
            }
            TelegramState::ValidateOTP(token) => {
                ui.label("Insert OTP here");
                ui.text_edit_singleline(&mut self.otp_field);
                if ui.button("Validate OTP").clicked() {
                    println!("OTP validation requested.");
                    self.spawner.spawn_task(Task {
                        task_type: TaskType::ValidateOTP(token.clone(), self.otp_field.clone()),
                        result: self.telegram_tx.clone(),
                    })
                }
            }
            TelegramState::LoggedIn => {
                self.main_flow(ui);
            }
        });
    }
}

impl TelegramGroupInfoApp {
    fn new() -> Self {
        let (send, mut recv) = tokio::sync::mpsc::channel(16);
        Self {
            spawner: TaskSpawner::new(),
            telegram_rx: recv,
            telegram_tx: send,
            // client: None,
            picked_path: None,
            chat_name: "".to_string(),
            phone_number: "".to_string(),
            otp_field: "".to_string(),
            result: Vec::new(),
            telegram_state: TelegramState::NeedOTP,
        }
    }
    /// Checks if we got something in the channel, and changes state in that case.
    fn check_async_updates(&mut self) {
        // if let TelegramState::LoggedIn = self.telegram_state {
        //     return;
        // }
        if let Ok(msg) = self.telegram_rx.try_recv() {
            match msg {
                TaskResult::OTP(otp_res) => match otp_res {
                    Some(None) => {
                        self.telegram_state = TelegramState::LoggedIn;
                    }
                    Some(Some(token)) => {
                        self.telegram_state = TelegramState::ValidateOTP(Arc::new(token))
                    }
                    None => {
                        println!("Fail");
                    }
                },
                TaskResult::ValidateOTP(Some(_)) => {
                    self.telegram_state = TelegramState::LoggedIn;
                }
                TaskResult::GetParticipantsResult(Some(value)) => {
                    println!("Got participants! {:?}", value.1);
                    self.result.push(value);
                }
                _ => {}
            }
        }
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
            ui.vertical(|ui| {
                ui.label("Picked folder:");
                ui.monospace(picked_path);
                if ui.button("Go").clicked() {
                    let lines: Vec<_> = self.chat_name.lines().collect();
                    for (i, chat_name) in lines.iter().enumerate() {
                        ui.monospace(format!("({}/{}) Doing {} ...", i, lines.len(), chat_name));
                        self.spawner.spawn_task(Task {
                            task_type: TaskType::GetParticipants(chat_name.to_string()),
                            result: self.telegram_tx.clone(),
                        });
                        // let maybe_participants =
                        //     get_participants(self.client.clone(), chat_name.to_string());
                        // match maybe_participants {
                        //     Ok(result) => {
                        //         self.result.push(result);
                        //     }
                        //     Err(x) => {}
                        // }
                    }
                }
            });
            ui.separator();
            for (i, result) in self.result.iter().enumerate() {
                let msg = format!("There are {} participants in the group {}", result.1, i);
                println!("{msg}");
                ui.label(msg);
                ui.separator();
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
    // let rt = tokio::runtime::Builder::new_current_thread()
    //     .enable_all()
    //     .build()
    //     .unwrap();

    // let client = rt.block_on(get_client()).unwrap();
    // rt.shutdown_background();

    let app = TelegramGroupInfoApp::new();

    eframe::run_native(
        "Pigna telegram scraper",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .expect("GUI crashed");
    // println!("{}", app.picked_path.clone().unwrap());
}
