use std::sync::Arc;

use grammers_client::types::*;
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;

use grammers_client::client::chats::ParticipantIter;

use tokio::runtime::Builder;
use tokio::sync::mpsc;

pub enum TaskType {
    RequestOTP(String), // phone number
    ValidateOTP(Arc<LoginToken>, String), // auth_token, otp
                        // GetParticipants(String),         // group name
}
pub struct Task {
    // info that describes the task
    pub task_type: TaskType,
    pub client: Client,
    pub result: mpsc::Sender<TaskResult>, // channel for result
}
// impl Task {
//     pub fn new(task_type: TaskType, client: Client, result: ) -> Self {
//         Self { task_type, client }
//     }
// }

pub enum TaskResult {
    OTP(Option<Option<LoginToken>>),
    ValidateOTP(Option<Client>),
    // GetParticipants(TelegramResult<(ParticipantIter, usize)>),
}

async fn handle_task(task: Task) {
    match task.task_type {
        TaskType::RequestOTP(phone) => {
            task.result
                .send(TaskResult::OTP(
                    request_login_code(task.client, &phone).await.ok(),
                ))
                .await
                .expect("channel send fail");
        }
        TaskType::ValidateOTP(token, otp) => {
            task.result
                .send(TaskResult::ValidateOTP(
                    login(task.client, &token, &otp).await.ok(),
                ))
                .await
                .expect("channel send fail");
        } // TaskType::GetParticipants(chat_name) => {
          //     task.result.send(TaskResult::GetParticipants(
          //         get_participants(task.client, chat_name).await,
          //     ));
          // }
    }
}

#[derive(Clone)]
pub struct TaskSpawner {
    spawn: mpsc::Sender<Task>,
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

        std::thread::spawn(move || {
            rt.block_on(async move {
                while let Some(task) = recv.recv().await {
                    tokio::spawn(handle_task(task));
                }

                // Once all senders have gone out of scope,
                // the `.recv()` call returns None and it will
                // exit from the while loop and shut down the
                // thread.
            });
        });

        Self { spawn: send }
    }

    pub fn spawn_task(&self, task: Task) {
        match self.spawn.blocking_send(task) {
            Ok(()) => {}
            Err(_) => panic!("The shared runtime has shut down."),
        }
    }
}

const SESSION_FILE: &str = "downloader.session";

type TelegramResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main]
pub async fn get_client() -> TelegramResult<Client> {
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
    return Ok(client);
}

pub async fn request_login_code(client: Client, phone: &str) -> TelegramResult<Option<LoginToken>> {
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

pub async fn login(client: Client, token: &LoginToken, code: &str) -> TelegramResult<Client> {
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
    return Ok(client);
}

#[tokio::main]
pub async fn get_participants(
    client: Client,
    chat_name: String,
) -> TelegramResult<(ParticipantIter, usize)> {
    let maybe_chat = client.resolve_username(chat_name.as_str()).await?;

    let chat = maybe_chat.unwrap_or_else(|| panic!("Chat {} could not be found", chat_name));

    let mut participants = client.iter_participants(&chat);
    let total_participants = participants.total().await.unwrap();

    Ok((participants, total_participants))
}

// fn prompt(message: &str) -> TelegramResult<String> {
//     let stdout = io::stdout();
//     let mut stdout = stdout.lock();
//     stdout.write_all(message.as_bytes())?;
//     stdout.flush()?;

//     let stdin = io::stdin();
//     let mut stdin = stdin.lock();

//     let mut line = String::new();
//     stdin.read_line(&mut line)?;
//     Ok(line)
// }
