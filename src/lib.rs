use std::io;
use std::io::{BufRead, Write};

use grammers_client::types::Media::{Contact, Document, Photo, Sticker};
use grammers_client::types::*;
use grammers_client::{Client, Config, SignInError};
use grammers_session::Session;

use grammers_client::client::chats::ParticipantIter;

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

pub async fn login(client: Client, phone: String) -> TelegramResult<Client> {
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
