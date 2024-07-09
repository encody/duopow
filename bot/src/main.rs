use std::{path::PathBuf, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use dptree::{case, deps};
use ethers::{
    contract::abigen,
    core::k256::ecdsa::SigningKey,
    middleware::SignerMiddleware,
    providers::Middleware,
    signers::{Signer, Wallet},
    types::{Address, U256},
};
use log::Level;
use once_cell::sync::Lazy;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    prelude::*,
    utils::command::BotCommands,
};

const USER_AGENT: &str = concat!("duopow-bot/", env!("CARGO_PKG_VERSION"));

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

        #[clap(short, long, env = "DUOPOW_RPC")]
        rpc: Url,
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
    #[command(description = "link your Duolingo and Taiko accounts (do this first)")]
    Link,
    #[command(
        description = "[username] register your Duolingo account with the smart contract (do this second)"
    )]
    Register { username: String },
    #[command(description = "[username] unregister your Duolingo account")]
    Unregister { username: String },
    #[command(description = "[username] update your XP and mint your rewards")]
    Update { username: String },
    #[command(description = "[username] view an account")]
    Check { username: String },
    #[command(description = "cancel")]
    Cancel,
}

async fn get_user_total_xp(http: &reqwest::Client, uid: u64) -> anyhow::Result<u64> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TotalXp {
        total_xp: u64,
    }

    Ok(http
        .get(format!("https://www.duolingo.com/2017-06-30/users/{uid}"))
        .query(&[("fields", "totalXp")])
        .send()
        .await?
        .json::<TotalXp>()
        .await?
        .total_xp)
}

async fn get_user_uid_and_maybe_address(
    http: &reqwest::Client,
    username: &str,
) -> Option<(u64, Option<Address>)> {
    let response = get_user_by_username(http, username).await.ok()?;

    let uid = response.id;

    let address_match = ETH_ADDRESS.find(&response.bio)?;

    let address: Option<Address> = address_match.as_str().parse().ok();

    Some((uid, address))
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
            rpc,
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

            let provider =
                ethers::providers::Provider::<ethers::providers::Http>::try_from(rpc.as_str())
                    .unwrap();

            let chain_id = provider.get_chainid().await.unwrap().as_u64();

            let duo = DuolingoPowContract::new(
                contract,
                Arc::new(SignerMiddleware::new(
                    provider,
                    wallet.with_chain_id(chain_id),
                )),
            );

            Dispatcher::builder(bot, handler())
                .dependencies(deps![
                    Arc::new(Connections {
                        http,
                        contract: duo,
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
    LinkReceiveUsername,
    LinkReceiveAddress {
        username: String,
    },
    LinkReceiveJwt {
        username: String,
        address: Address,
    },
}

struct Connections {
    http: reqwest::Client,
    contract: DuolingoPowContract<
        SignerMiddleware<ethers::providers::Provider<ethers::providers::Http>, Wallet<SigningKey>>,
    >,
}

fn handler() -> UpdateHandler<anyhow::Error> {
    dialogue::enter::<Update, InMemStorage<ChatState>, _, _>().branch(
        Update::filter_message()
            .branch(
                teloxide::filter_command::<BotCommand, _>().branch(
                    case![ChatState::Start]
                        .branch(case![BotCommand::Help].endpoint(help))
                        .branch(case![BotCommand::Cancel].endpoint(cancel))
                        .branch(case![BotCommand::Link].endpoint(begin_link))
                        .branch(case![BotCommand::Register { username }].endpoint(register))
                        .branch(case![BotCommand::Update { username }].endpoint(update))
                        .branch(case![BotCommand::Check { username }].endpoint(check))
                        .branch(case![BotCommand::Unregister { username }].endpoint(unregister)),
                ),
            )
            .branch(case![ChatState::LinkReceiveUsername].endpoint(link_receive_username))
            .branch(
                case![ChatState::LinkReceiveAddress { username }].endpoint(link_receive_address),
            )
            .branch(
                case![ChatState::LinkReceiveJwt { username, address }].endpoint(link_receive_jwt),
            ),
    )
}

async fn check(
    bot: Bot,
    msg: Message,
    connections: Arc<Connections>,
    username: String,
) -> anyhow::Result<()> {
    let loading_msg = bot
        .send_message(msg.chat.id, "Okay, loading your Duolingo profile...")
        .await?;

    let Some((uid, address_in_profile)) =
        get_user_uid_and_address(&connections.http, &username).await
    else {
        bot.delete_message(msg.chat.id, loading_msg.id).await?;
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };

    let total_xp = get_user_total_xp(&connections.http, uid).await?;

    let (address_in_contract, xp_in_contract): (Address, U256) =
        connections.contract.users(uid.into()).await?;

    let xp_to_mint = total_xp - xp_in_contract.as_u64();

    if address_in_contract != address_in_profile {
        bot.send_message(msg.chat.id, format!(
            "It looks like your address has changed. You've registered to withdraw to {}, but your Duolingo profile has {}.",
            ethers::utils::to_checksum(&address_in_contract, None),
            ethers::utils::to_checksum(&address_in_profile, None),
        )).await?;
    }

    bot.send_message(
        msg.chat.id,
        format!(
            "Your account has registered the address {}, and you can mint {xp_to_mint} XP as POD.",
            ethers::utils::to_checksum(&address_in_contract, None)
        ),
    )
    .await?;
    bot.delete_message(msg.chat.id, loading_msg.id).await?;

    Ok(())
}

async fn update(
    bot: Bot,
    msg: Message,
    connections: Arc<Connections>,
    username: String,
) -> anyhow::Result<()> {
    let loading_msg = bot
        .send_message(msg.chat.id, "Okay, loading your Duolingo profile...")
        .await?;

    let (uid, _address) = get_user_uid_and_address(&connections.http, &username)
        .await
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    let total_xp = get_user_total_xp(&connections.http, uid).await?;

    bot.send_message(msg.chat.id, format!("Wow, you have {total_xp} XP!"))
        .await?;
    bot.delete_message(msg.chat.id, loading_msg.id).await?;

    let sending_msg = bot
        .send_message(msg.chat.id, "Minting your rewards...")
        .await?;

    let (_address_in_contract, xp_in_contract): (Address, U256) =
        connections.contract.users(uid.into()).await?;

    log::log!(Level::Info, "XP in contract: {}", xp_in_contract.as_u128());

    if xp_in_contract == total_xp.into() {
        bot.send_message(msg.chat.id, "You need to earn more XP to receive rewards.")
            .await?;
        bot.delete_message(msg.chat.id, sending_msg.id).await?;
        return Ok(());
    }

    connections
        .contract
        .report_xp(uid.into(), total_xp.into())
        .send()
        .await?;

    bot.send_message(
        msg.chat.id,
        format!(
            "Congratulations, you received {} POD!",
            (U256::from(total_xp) - xp_in_contract).as_u64()
        ),
    )
    .await?;
    bot.delete_message(msg.chat.id, sending_msg.id).await?;

    Ok(())
}

async fn unregister(
    bot: Bot,
    msg: Message,
    connections: Arc<Connections>,
    username: String,
) -> anyhow::Result<()> {
    let loading_msg = bot
        .send_message(msg.chat.id, "Okay, loading your Duolingo profile...")
        .await?;

    let (uid, _address) = get_user_uid_and_address(&connections.http, &username)
        .await
        .ok_or_else(|| anyhow::anyhow!("User not found"))?;

    let unregistering_msg = bot
        .send_message(msg.chat.id, "Unregistering you from the contract...")
        .await?;
    bot.delete_message(msg.chat.id, loading_msg.id).await?;

    connections
        .contract
        .user_unregister(uid.into())
        .send()
        .await?;

    bot.send_message(
        msg.chat.id,
        "You've been unregistered. Sorry to see you go!",
    )
    .await?;
    bot.delete_message(msg.chat.id, unregistering_msg.id)
        .await?;

    Ok(())
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

    let checking_registration_msg = bot
        .send_message(msg.chat.id, "Found you! Checking your registration...")
        .await?;
    bot.delete_message(msg.chat.id, loading_msg.id).await?;

    let ((address_from_contract, _xp_from_contract), xp_from_duolingo) = tokio::try_join!(
        async {
            let r: (Address, U256) = connections.contract.users(uid.into()).await?;
            Ok(r)
        },
        async { get_user_total_xp(&connections.http, uid).await },
    )?;

    if address_from_contract.is_zero() {
        let registration_msg = bot
            .send_message(
                msg.chat.id,
                format!("Registering ${address} with the contract..."),
            )
            .await?;
        bot.delete_message(msg.chat.id, checking_registration_msg.id)
            .await?;

        connections
            .contract
            .user_register(uid.into(), address, xp_from_duolingo.into())
            .send()
            .await?;

        bot.send_message(msg.chat.id, "Registered!").await?;
        bot.delete_message(msg.chat.id, registration_msg.id).await?;
    } else if address_from_contract != address {
        let update_msg = bot
            .send_message(msg.chat.id, "Looks like we need to update your profile...")
            .await?;
        bot.delete_message(msg.chat.id, checking_registration_msg.id)
            .await?;

        connections
            .contract
            .user_update_address(uid.into(), address)
            .send()
            .await?;

        bot.delete_message(msg.chat.id, update_msg.id).await?;
        bot.send_message(msg.chat.id, "Updated!").await?;
    } else {
        bot.send_message(msg.chat.id, "Already registered!").await?;
        bot.delete_message(msg.chat.id, checking_registration_msg.id)
            .await?;
    }

    Ok(())
}

async fn begin_link(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<ChatState, InMemStorage<ChatState>>,
) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, "Let's get your Duolingo account set up.")
        .await?;
    bot.send_message(msg.chat.id, "First, what's your username?")
        .await?;

    dialogue.update(ChatState::LinkReceiveUsername).await?;

    Ok(())
}

async fn link_receive_username(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<ChatState, InMemStorage<ChatState>>,
    connections: Arc<Connections>,
) -> anyhow::Result<()> {
    if let Some(text) = msg.text() {
        let found_user = get_user_uid_and_maybe_address(&connections.http, text).await;
        if let Some((_uid, address)) = found_user {
            bot.send_message(msg.chat.id, "Great to meet you!").await?;
            bot.send_message(msg.chat.id, "Now, we need to link your profile.")
                .await?;
            if let Some(address) = address {
                bot.send_message(
                    msg.chat.id,
                    format!("It looks like your profile is already linked to {address}."),
                )
                .await?;
            }

            dialogue
                .update(ChatState::LinkReceiveAddress {
                    username: text.to_owned(),
                })
                .await?;

            bot.send_message(msg.chat.id, "What is your Taiko address?")
                .await?;
        } else {
            bot.send_message(msg.chat.id, "User not found. Please try again.")
                .await?;
        }
    } else {
        bot.send_message(msg.chat.id, "Please send a username.")
            .await?;
    }

    Ok(())
}

async fn link_receive_address(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<ChatState, InMemStorage<ChatState>>,
    username: String,
) -> anyhow::Result<()> {
    if let Some(address) = msg.text() {
        let address = ethers::utils::parse_checksummed(address, None);

        if let Ok(address) = address {
            dialogue
                .update(ChatState::LinkReceiveJwt { username, address })
                .await?;

            bot.send_message(msg.chat.id, "Okay, now please send your JWT. You can find instructions for how to get it here: https://github.com/encody/duopow")
                .await?;
        } else {
            bot.send_message(msg.chat.id, "Invalid address. Please try again.")
                .await?;
        }
    } else {
        bot.send_message(msg.chat.id, "Please send an address.")
            .await?;
    }

    Ok(())
}

async fn link_receive_jwt(
    bot: Bot,
    msg: Message,
    dialogue: Dialogue<ChatState, InMemStorage<ChatState>>,
    connections: Arc<Connections>,
    (_username, address): (String, Address),
) -> anyhow::Result<()> {
    if let Some(jwt) = msg.text() {
        bot.send_message(msg.chat.id, "Got it! Linking profile...")
            .await?;
        bot.delete_message(msg.chat.id, msg.id).await?;
        add_address_to_profile(&connections.http, jwt, address).await?;
        dialogue.update(ChatState::Start).await?;
        bot.send_message(msg.chat.id, "Profile linked!").await?;
    } else {
        bot.send_message(msg.chat.id, "Please send a JWT.").await?;
    }

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

async fn cancel(
    bot: Bot,
    dialogue: Dialogue<ChatState, InMemStorage<ChatState>>,
    msg: Message,
) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, "Cancelling.").await?;

    dialogue.update(ChatState::Start).await?;
    Ok(())
}

async fn help(bot: Bot, msg: Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, BotCommand::descriptions().to_string())
        .await?;
    Ok(())
}

async fn get_user_by_uid(
    http: &reqwest::Client,
    uid: u64,
    jwt: &str,
) -> anyhow::Result<UserResponse> {
    let response = http
        .get(format!("https://www.duolingo.com/2017-06-30/users/{uid}"))
        .header("Host", "www.duolingo.com")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:127.0) Gecko/20100101 Firefox/127.0",
        )
        .bearer_auth(jwt)
        .send()
        .await?;

    let user_response = response.json::<UserResponse>().await?;

    Ok(user_response)
}

fn get_uid_from_jwt(token: &str) -> u64 {
    #[derive(Deserialize)]
    struct Sub {
        sub: u64,
    }

    let sub = serde_json::from_slice::<Sub>(
        &base64::Engine::decode(
            &base64::prelude::BASE64_STANDARD_NO_PAD,
            token.split('.').nth(1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
    .sub;

    sub
}

async fn add_address_to_profile(
    http: &reqwest::Client,
    jwt: &str,
    address: Address,
) -> anyhow::Result<()> {
    let uid = get_uid_from_jwt(jwt);
    let original_bio = get_user_by_uid(http, uid, jwt).await.unwrap().bio;
    let address_str = ethers::utils::to_checksum(&address, None);
    let new_bio = if ETH_ADDRESS.is_match(&original_bio) {
        ETH_ADDRESS.replace(&original_bio, address_str)
    } else {
        std::borrow::Cow::Owned(format!("{} {}", original_bio, address_str))
    };

    // panic!("{}", new_bio);

    // send update
    http.patch(format!("https://www.duolingo.com/2017-06-30/users/{uid}"))
        .query(&[("fields", "bio")])
        .bearer_auth(jwt)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:127.0) Gecko/20100101 Firefox/127.0",
        )
        .header("Referer", "https://www.duolingo.com/settings/profile")
        .json(&json!({
            "bio": new_bio,
        }))
        .send()
        .await
        .unwrap();

    Ok(())
}
