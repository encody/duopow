use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use dptree::{case, deps};
use ethers::{contract::abigen, core::k256::ecdsa::SigningKey, signers::Wallet, types::Address};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    prelude::*,
    utils::command::BotCommands,
};

const USER_AGENT: &str = concat!("duopow-bot/", env!("CARGO_PKG_VERSION"));
const CHAIN_ID: u64 = 167000; // Taiko mainnet

abigen!(
    DuolingoPowContract,
    "../contract/out/DuolingoPow.sol/DuolingoPow.json"
);

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
    // UpdateProfile {
    //     address: Address,

    //     #[clap(short, long, env = "DUOPOW_JWT")]
    //     jwt: String,
    // },
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
#[serde(rename_all = "camelCase")]
struct CourseResponse {
    title: String,
    learning_language: String,
    xp: u64,
    from_language: String,
    id: String,
}

// async fn get_user_by_uid(http: &reqwest::Client, uid: u64) -> anyhow::Result<UserResponse> {
//     let response = http
//         .get(format!("https://www.duolingo.com/2017-06-30/users/{uid}"))
//         .header("Host", "www.duolingo.com")
//         .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:127.0) Gecko/20100101 Firefox/127.0")
//         .send()
//         .await?;

//     panic!("{}", response.text().await.unwrap());

//     // let user_response = response
//     //     .json::<UserResponse>()
//     //     .await?;

//     // Ok(user_response)
// }

async fn get_user_by_username(
    http: &reqwest::Client,
    username: &str,
) -> anyhow::Result<UserResponse> {
    #[derive(Deserialize)]
    struct UserRequestResponse {
        users: Vec<UserResponse>,
    }

    let mut response = http
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
    http: &reqwest::Client,
    username: &str,
) -> Option<(u64, Address)> {
    let response = get_user_by_username(http, username).await.ok()?;

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

            let wallet = Wallet::decrypt_keystore(keystore, password).unwrap();

            let http = reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap();

            Dispatcher::builder(bot, handler())
                .dependencies(deps![
                    Arc::new(Connections {
                        http,
                        wallet,
                        contract
                    }),
                    InMemStorage::<ChatState>::new()
                ])
                .error_handler(LoggingErrorHandler::with_custom_text(
                    "An error has occurred in the dispatcher",
                ))
                .enable_ctrlc_handler()
                .build()
                .dispatch()
                .await;
        }
    }
}

#[derive(Clone, Default)]
enum ChatState {
    #[default]
    Start,
}

struct Connections {
    http: reqwest::Client,
    wallet: Wallet<SigningKey>,
    contract: Address,
}

fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<ChatState>, _, _>().branch(
        Update::filter_message().branch(
            teloxide::filter_command::<BotCommand, _>().branch(
                case![ChatState::Start]
                    .branch(case![BotCommand::Help].endpoint(help))
                    .branch(case![BotCommand::Register { username }].endpoint(register)),
            ),
        ),
    )
}

async fn register(
    bot: Bot,
    msg: Message,
    connections: Arc<Connections>,
    username: String,
) -> anyhow::Result<()> {
    let loading_msg = bot
        .send_message(msg.chat.id, "Okay, loading your Duolingo profile...")
        .await?;

    let (uid, address) = get_user_uid_and_address(&connections.http, &username)
        .await
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    bot.send_message(msg.chat.id, "Found you! Checking your registration...")
        .await?;
    bot.delete_message(loading_msg.chat.id, loading_msg.id)
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_rpc() {
    let rpc = ethers::providers::Provider::try_from("https://rpc.mainnet.taiko.xyz/").unwrap();
    let addy: Address = "0x7d02A3E0180451B17e5D7f29eF78d06F8117106C"
        .parse()
        .unwrap();
    let duo = DuolingoPowContract::new(addy, Arc::new(rpc));
    let b = duo
        .balance_of(
            "0x69AA0361Dbb0527d4F1e5312403Bd41788fe61Fe"
                .parse()
                .unwrap(),
        )
        .await
        .unwrap();
    println!("{b:?}");
}

async fn help(bot: Bot, msg: Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, BotCommand::descriptions().to_string())
        .await?;
    Ok(())
}

// fn get_uid_from_jwt(token: &str) -> u64 {
//     #[derive(Deserialize)]
//     struct Sub {
//         sub: u64,
//     }

//     let sub = serde_json::from_slice::<Sub>(
//         &base64::prelude::BASE64_STANDARD_NO_PAD
//             .decode(token.split('.').nth(1).unwrap())
//             .unwrap(),
//     )
//     .unwrap()
//     .sub;

//     sub
// }

// async fn add_address_to_profile(
//     http: &reqwest::Client,
//     jwt: &str,
//     address: Address,
// ) -> anyhow::Result<()> {
//     let uid = get_uid_from_jwt(jwt);
//     let original_bio = get_user_by_uid(http, uid).await.unwrap().bio;
//     let new_bio = if ETH_ADDRESS.is_match(&original_bio) {
//         ETH_ADDRESS.replace(&original_bio, address.to_string())
//     } else {
//         Cow::Owned(format!("{} {}", original_bio, address))
//     };

//     panic!("{}", new_bio);

//     // send update
//     http.patch(format!("https://www.duolingo.com/2017-06-30/users/{uid}"))
//         .query(&[("fields", "bio")])
//         .bearer_auth(jwt)
//         .header(
//             "User-Agent",
//             "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:127.0) Gecko/20100101 Firefox/127.0",
//         )
//         .header("Referer", "https://www.duolingo.com/settings/profile")
//         .json(&json!({
//             "bio": new_bio,
//         }))
//         .send()
//         .await
//         .unwrap();

//     Ok(())

//     // todo!()

// }
