//! Example to download all messages and media from a chat.
//!
//! The `TG_ID` and `TG_HASH` environment variables must be set (learn how to do it for
//! [Windows](https://ss64.com/nt/set.html) or [Linux](https://ss64.com/bash/export.html))
//! to Telegram's API ID and API hash respectively.
//!
//! Then, run it as:
//!
//! ```sh
//! cargo run --example downloader -- CHAT_NAME
//! ```
//!
//! Messages will be printed to stdout, and media will be saved in the `target/` folder locally, named
//! message-[MSG_ID].[EXT]

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use grammers_client::client::chats::ParticipantIter;

use std::io;
use std::io::{BufRead, Write};

use grammers_client::{Client, Config, SignInError};
// use mime_guess::mime;
use simple_logger::SimpleLogger;
use tokio::runtime;

use grammers_client::types::Media::{Contact, Document, Photo, Sticker};
use grammers_client::types::*;
use grammers_session::Session;

const SESSION_FILE: &str = "downloader.session";

#[derive(Default)]
struct MyApp {
    picked_path: Option<String>,
    chat_id: String,
    result: Option<(ParticipantIter, usize)>,
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("Telegram chat name:");
            if ui.text_edit_singleline(&mut self.chat_id).changed() {
                println!("Changed: {}", self.chat_id);
            }
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
                        let maybe_participants = runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .expect("Tokio fail")
                            .block_on(get_participants(self.chat_id.clone()));
                        match maybe_participants {
                            Ok(result) => {
                                self.result = Some(result);
                            }
                            Err(x) => {}
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
        });
    }
}

type TelegramResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
async fn get_participants(chat_name: String) -> TelegramResult<(ParticipantIter, usize)> {
    let api_id = env!("TG_ID").parse().expect("TG_ID invalid");
    let api_hash = env!("TG_HASH").to_string();

    // let chat_name = env::args().nth(1).expect("chat name missing");

    println!("Connecting to Telegram...");
    let client = Client::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
    .await?;
    println!("Connected!");

    // If we can't save the session, sign out once we're done.
    let mut sign_out = false;

    if !client.is_authorized().await? {
        println!("Signing in...");
        let phone = prompt("Enter your phone number (international format): ")?;
        let token = client.request_login_code(&phone).await?;
        let code = prompt("Enter the code you received: ")?;
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(password_token)) => {
                // Note: this `prompt` method will echo the password in the console.
                //       Real code might want to use a better way to handle this.
                let hint = password_token.hint().unwrap();
                let prompt_message = format!("Enter the password (hint {}): ", &hint);
                let password = prompt(prompt_message.as_str())?;

                client
                    .check_password(password_token, password.trim())
                    .await?;
            }
            Ok(_) => (),
            Err(e) => panic!("{}", e),
        };
        println!("Signed in!");
        match client.session().save_to_file(SESSION_FILE) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "NOTE: failed to save the session, will sign out when done: {}",
                    e
                );
                sign_out = true;
            }
        }
    }

    let maybe_chat = client.resolve_username(chat_name.as_str()).await?;

    let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {} could not be found", chat_name));

    let mut participants = client.iter_participants(&chat);
    let total_participants = participants.total().await.unwrap();

    Ok((participants, total_participants))
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
    eframe::run_native("Boh", options, Box::new(|_cc| Box::new(app))).expect("GUI crashed");
}

fn prompt(message: &str) -> TelegramResult<String> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(message.as_bytes())?;
    stdout.flush()?;

    let stdin = io::stdin();
    let mut stdin = stdin.lock();

    let mut line = String::new();
    stdin.read_line(&mut line)?;
    Ok(line)
}
