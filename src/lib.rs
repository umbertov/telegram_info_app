use std::fmt::format;
use std::io;
use std::sync::{Arc, Mutex};

use grammers_client::types::*;
use grammers_client::{Client as GrammersClient, Config, SignInError};
use grammers_session::Session;

use grammers_client::client::chats::ParticipantIter;

use tokio::runtime::Builder;
use tokio::sync::mpsc;

type Client = Arc<Mutex<GrammersClient>>;

pub enum TaskType {
    RequestOTP(String),                   // phone number
    ValidateOTP(Arc<LoginToken>, String), // auth_token, otp
    GetParticipants(String),              // group name
    WriteCSV(String, ParticipantIter),    // group name, pariticipants
}
pub struct Task {
    // info that describes the task
    pub task_type: TaskType,
    // pub client: Client,
    pub result: mpsc::Sender<TaskResult>, // channel for result
}

pub enum TaskResult {
    OTP(Option<Option<LoginToken>>),
    ValidateOTP(Option<()>),
    GetParticipantsResult(String, Option<(ParticipantIter, usize)>),
    WriteCSVResult(String, bool),
}

async fn handle_task(task: Task, client: GrammersClient) {
    match task.task_type {
        TaskType::RequestOTP(phone) => {
            println!("Got RequestOTP request");
            task.result
                .send(TaskResult::OTP(get_login_code(client, &phone).await.ok()))
                .await
                .expect("channel send fail");
        }
        TaskType::ValidateOTP(token, otp) => {
            task.result
                .send(TaskResult::ValidateOTP(
                    login(client, &token, &otp).await.ok(),
                ))
                .await
                .expect("channel send fail");
        }
        TaskType::GetParticipants(chat_name) => {
            let mut participants = get_participants(client, chat_name.clone())
                .await
                .expect("bojh");
            handle_write_csv(chat_name.clone(), &mut participants.0, task.result.clone()).await;
            task.result
                .send(TaskResult::GetParticipantsResult(
                    chat_name.clone(),
                    Some(participants),
                ))
                .await
                .expect("channel send fail");
        }
        TaskType::WriteCSV(group_name, mut participants) => {
            // handle_write_csv(group_name, &mut participants, task.result).await;
        }
    }
}

use serde::Serialize;
#[derive(Debug, Serialize)]
struct ParticipantRow<'a> {
    username: Option<&'a str>,
    first_name: &'a str,
    last_name: Option<&'a str>,
    scam: bool,
    phone: Option<&'a str>,
    verified: bool,
    is_bot: bool,
    is_support: bool,
    role: &'a str, // role: Option<&'a str>,
}

async fn handle_write_csv(
    group_name: String,
    participants: &mut ParticipantIter,
    result: mpsc::Sender<TaskResult>,
) {
    let participants = participants_to_vec(participants).await;
    let mut csv_writer = csv::Writer::from_writer(
        std::fs::File::create(format!("{}.csv", group_name)).expect("Couldnt open for writing"),
    );
    for participant in participants.iter() {
        let participant = ParticipantRow {
            username: participant.user.username(),
            first_name: participant.user.first_name(),
            last_name: participant.user.last_name(),
            scam: participant.user.scam(),
            phone: participant.user.phone(),
            verified: participant.user.verified(),
            is_bot: participant.user.is_bot(),
            is_support: participant.user.support(),
            role: &format!("{:?}", participant.role),
        };

        println!("Writing {:?}", participant);

        csv_writer.serialize(participant).unwrap();
    }
    result
        .send(TaskResult::WriteCSVResult(group_name, true))
        .await
        .unwrap();
}
async fn participants_to_vec(participants: &mut ParticipantIter) -> Vec<Participant> {
    let mut i = 0;
    let total = participants.total().await.unwrap();
    let mut out = Vec::new();
    loop {
        i += 1;
        println!("Downlaod [{}/{}]...", i, total);
        match participants.next().await {
            Ok(Some(participant)) => {
                out.push(participant);
                // do stuff
            }
            Ok(None) => {
                break; // normal finish
            }
            Err(msg) => {
                println!("Fail {}", msg);
                break; // error
            }
        }
    }
    return out;
}

#[derive(Clone)]
pub struct TaskSpawner {
    spawn: mpsc::Sender<Task>,
    client: Client,
}

impl TaskSpawner {
    pub fn new() -> TaskSpawner {
        // Set up a channel for communicating.
        let (send, mut recv) = mpsc::channel(16);

        // Build the runtime for the new thread.
        //
        // The runtime is created before spawning the thread
        // to more cleanly forward errors if the `unwrap()`
        // panics.
        let rt = Builder::new_current_thread().enable_all().build().unwrap();
        let client = Arc::new(Mutex::new(rt.block_on(get_client()).unwrap()));
        let c2 = client.clone();

        std::thread::spawn(move || {
            rt.block_on(async move {
                while let Some(task) = recv.recv().await {
                    // tokio::spawn(handle_task(task, client.lock().unwrap().clone()));
                    tokio::spawn(handle_task(task, c2.lock().unwrap().clone()));
                }

                // Once all senders have gone out of scope,
                // the `.recv()` call returns None and it will
                // exit from the while loop and shut down the
                // thread.
            });
        });

        Self {
            spawn: send,
            client: client.clone(),
        }
    }

    pub fn spawn_task(&self, task: Task) {
        match self.spawn.blocking_send(task) {
            Ok(()) => {}
            Err(_) => panic!("The shared runtime has shut down."),
        }
    }
}

const SESSION_FILE: &str = "downloader.session";

pub type TelegramResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// #[tokio::main]
pub async fn get_client() -> TelegramResult<GrammersClient> {
    let api_id = env!("TG_ID").parse().expect("TG_ID invalid");
    let api_hash = env!("TG_HASH").to_string();

    // let chat_name = env::args().nth(1).expect("chat name missing");

    println!("Connecting to Telegram...");
    let client = GrammersClient::connect(Config {
        session: Session::load_file_or_create(SESSION_FILE)?,
        api_id,
        api_hash: api_hash.clone(),
        params: Default::default(),
    })
    .await?;
    println!("Connected!");
    return Ok(client);
}

pub async fn get_login_code(
    client: GrammersClient,
    phone: &str,
) -> TelegramResult<Option<LoginToken>> {
    println!("Checking if authorized...");
    if !client.is_authorized().await? {
        println!("No. Request login code");
        let token = client.request_login_code(&phone).await?;
        return Ok(Some(token));
    } else {
        println!("Yes");
        return Ok(None);
    }
}

pub async fn login(client: GrammersClient, token: &LoginToken, code: &str) -> TelegramResult<()> {
    if !client.is_authorized().await? {
        let signed_in = client.sign_in(&token, &code).await;
        match signed_in {
            Err(SignInError::PasswordRequired(_password_token)) => {
                panic!("Password requested")
                // // Note: this `prompt` method will echo the password in the console.
                // //       Real code might want to use a better way to handle this.
                // let hint = password_token.hint().unwrap();
                // let prompt_message = format!("Enter the password (hint {}): ", &hint);
                // let password = prompt(prompt_message.as_str())?;

                // client
                //     .check_password(password_token, password.trim())
                //     .await?;
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
            }
        }
    }
    return Ok(());
}

pub async fn get_participants(
    client: GrammersClient,
    chat_name: String,
) -> TelegramResult<(ParticipantIter, usize)> {
    let maybe_chat = client.resolve_username(chat_name.as_str()).await?;

    let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {} could not be found", chat_name));

    let mut participants = client.iter_participants(&chat);
    let total_participants = participants.total().await.unwrap();

    Ok((participants, total_participants))
}
