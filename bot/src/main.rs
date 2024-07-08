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
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use teloxide::{
    dispatching::{dialogue::InMemStorage, UpdateHandler},
    prelude::*,
    utils::command::BotCommands,
};
use tokio::sync::{Mutex, RwLock};

const USER_AGENT: &str = concat!("duopow-bot/", env!("CARGO_PKG_VERSION"));
const CHAIN_ID: u64 = 167000; // Taiko mainnet

static ETH_ADDRESS: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"0x[0-9a-fA-F]{40}").unwrap());

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

async fn get_user_from_duolingo_api(
    client: impl AsRef<reqwest::Client>,
    username: &str,
) -> anyhow::Result<UserResponse> {
    #[derive(Deserialize)]
    struct UserRequestResponse {
        users: Vec<UserResponse>,
    }

    let mut response = client
        .as_ref()
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
    #[command(description = "update your XP")]
    Update,
}

async fn get_user_uid_and_address(
    client: impl AsRef<reqwest::Client>,
    username: &str,
) -> Option<(u64, Address)> {
    let response = get_user_from_duolingo_api(client, username).await.ok()?;

    let uid = response.id;

    let address_match = ETH_ADDRESS.find(&response.bio)?;

    let address: Address = address_match.as_str().parse().ok()?;

    Some((uid, address))
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

            let http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap();

            // Dispatcher::builder(bot, handler)
            // Update

            Dispatcher::builder(bot, handler())
                .dependencies(deps![Connections { http }, InMemStorage::<()>::new()]);

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

struct ChatState {}

struct Connections {
    http: reqwest::Client,
}

fn handler() -> UpdateHandler<anyhow::Error> {
    todo!()
}
