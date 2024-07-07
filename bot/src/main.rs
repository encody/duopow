use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use clap::{Parser, Subcommand};
use dptree::deps;
use ethers::{
    core::{
        k256::{ecdsa::SigningKey, Secp256k1},
        rand::rngs::OsRng,
    },
    signers::{
        coins_bip39::{English, Mnemonic},
        MnemonicBuilder, Signer, Wallet,
    },
    types::Address,
};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, Executor};
use teloxide::{
    dispatching::{dialogue::InMemStorage, UpdateHandler},
    prelude::*,
    utils::command::BotCommands,
};
use tokio::sync::{Mutex, RwLock};

const USER_AGENT: &str = concat!("duopow-bot/", env!("CARGO_PKG_VERSION"));
const CHAIN_ID: u64 = 167000; // Taiko mainnet

#[derive(Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    GenerateKeystore {
        #[clap(short, long, default_value = "./keystore/")]
        dir: PathBuf,

        #[clap(short, long, env = "DUOPOW_PASSWORD", default_value = "")]
        password: String,
    },
    Run {
        #[clap(short, long, env = "DUOPOW_KEYSTORE")]
        keystore: PathBuf,

        #[clap(short, long, env = "DUOPOW_PASSWORD", default_value = "")]
        password: String,

        #[clap(short, long, env = "DUOPOW_TG_TOKEN")]
        tg_token: String,

        #[clap(short, long, env = "DUOPOW_CONTRACT")]
        contract: Address,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserResponse {
    streak: u32,
    id: u64,
    username: String,
    bio: String,
    name: String,
    courses: Vec<CourseResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CourseResponse {
    title: String,
    learning_language: String,
    xp: u64,
    from_language: String,
    id: String,
}

async fn get_user(client: &reqwest::Client, username: &str) -> anyhow::Result<UserResponse> {
    #[derive(Deserialize)]
    struct UserRequestResponse {
        users: Vec<UserResponse>,
    }

    let mut response = client
        .get("https://www.duolingo.com/2017-06-30/users")
        .query(&[("username", username)])
        .send()
        .await?
        .json::<UserRequestResponse>()
        .await?;

    if let Some(user) = response.users.pop() {
        Ok(user)
    } else {
        anyhow::bail!("User not found")
    }
}

#[derive(BotCommands, Clone)]
#[command(
    rename_rule = "lowercase",
    description = "These commands are supported:"
)]
enum BotCommand {
    #[command(description = "display this text again")]
    Help,
    #[command(description = "register your Duolingo account")]
    Register { username: String },
    #[command(description = "verify your Duolingo account")]
    Verify,
    #[command(description = "update your XP")]
    Update,
}

async fn create_user_wallet(
    db: &mut sqlx::SqliteConnection,
    uid: u32,
    username: &str,
) -> Wallet<SigningKey> {
    let w = ethers::signers::Wallet::new(&mut OsRng).with_chain_id(CHAIN_ID);
    let b = w.signer().to_bytes();
    let h = ethers::utils::hex::encode(b);

    sqlx::query!(
        "insert into users (id, username, signing_key_hex, verified) values (?, ?, ?, false)",
        uid,
        username,
        h
    )
    .execute(db)
    .await
    .unwrap();

    w
}

async fn is_user_verified(db: &mut sqlx::SqliteConnection, uid: u32) -> bool {
    sqlx::query!(
        "select verified from users where id = ? and verified is true limit 1",
        uid
    )
    .fetch_one(db)
    .await
    .is_ok()
}

async fn load_user_wallet(db: &mut sqlx::SqliteConnection, uid: u32) -> Option<Wallet<SigningKey>> {
    let h = sqlx::query!(
        "select signing_key_hex from users where id = ? limit 1",
        uid
    )
    .fetch_one(db)
    .await
    .ok()?
    .signing_key_hex;

    let k = SigningKey::from_slice(&ethers::utils::hex::decode(h).unwrap()).unwrap();
    let addr = ethers::utils::secret_key_to_address(&k);
    let w = Wallet::new_with_signer(k, addr, CHAIN_ID);

    Some(w)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let args = Args::parse();

    match args.command {
        Command::GenerateKeystore { dir, password } => {
            ethers::signers::LocalWallet::new_keystore(
                dir,
                &mut ethers::core::rand::rngs::OsRng,
                password,
                None,
            )
            .unwrap();
        }
        Command::Run {
            keystore,
            password,
            contract,
            tg_token,
        } => {
            pretty_env_logger::init();
            log::info!("Starting bot");

            let bot = Bot::with_client(
                tg_token,
                reqwest::Client::builder()
                    .user_agent(USER_AGENT)
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .unwrap(),
            );

            let mut db = sqlx::SqliteConnection::connect_with(
                &sqlx::sqlite::SqliteConnectOptions::new()
                    .filename("db.sqlite")
                    .create_if_missing(true),
            )
            .await
            .unwrap();

            sqlx::migrate!().run(&mut db).await.unwrap();

            let http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap();

            // Dispatcher::builder(bot, handler)
            // Update

            Dispatcher::builder(bot, handler()).dependencies(deps![
                Connections {
                    db: Arc::new(Mutex::new(db)),
                    http
                },
                InMemStorage::<()>::new()
            ]);

            todo!()

            // BotCommand::repl(bot, |bot: Bot, msg: Message, cmd: BotCommand| async move {
            //     match cmd {
            //         BotCommand::Help => {
            //             bot.send_message(msg.chat.id, BotCommand::descriptions().to_string())
            //                 .await?;
            //         }
            //         BotCommand::Register { username } => {}
            //         BotCommand::Verify => {}
            //         BotCommand::Update => {}
            //     }

            //     todo!()
            // });
        }
    }
}

struct ChatState {
    
}

struct Connections {
    db: Arc<Mutex<sqlx::SqliteConnection>>,
    http: reqwest::Client,
}

fn handler() -> UpdateHandler<anyhow::Error> {
    todo!()
}
