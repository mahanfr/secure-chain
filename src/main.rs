use std::{
    fs::{self},
    io::{self},
    sync::Arc,
};

use crate::{
    blockchain::Blockchain,
    clargs::parse_args,
    cli::Cli,
    keys::PublicKey,
    networking::{AppState, P2PNetwork},
    peers::bootstap_peers,
};
use anyhow::Result;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod block;
mod blockchain;
mod clargs;
mod cli;
mod errors;
mod keys;
mod logging;
mod networking;
mod peers;
mod protocol;
mod types;

static HISTORY_FOLDER: &str = "./data/.history";
static SHELL_HISTORY_LOC: &str = "./data/.history/shell.txt";

fn generate_files_if_needed() -> Result<()> {
    match fs::create_dir("./data") {
        Ok(_) => (),
        Err(e) => {
            if e.kind() != io::ErrorKind::AlreadyExists {
                panic!("Cannot Create Folder: {}", e);
            }
        }
    }
    match fs::create_dir(HISTORY_FOLDER) {
        Ok(()) => {
            let _ = fs::File::create(SHELL_HISTORY_LOC);
        }
        Err(e) => {
            if e.kind() != io::ErrorKind::AlreadyExists {
                panic!("cannot create folder: {}", e);
            }
        }
    }

    Ok(())
}

fn init_tracing() {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, "data/logs", "app.log");
    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(file_appender);

    let console_layer = fmt::layer()
        .with_target(true)
        .with_ansi(true);

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(console_layer)
        .with(file_layer);

    subscriber.try_init().ok();
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    init_tracing();
    generate_files_if_needed().unwrap();
    let args: Vec<String> = std::env::args().collect();

    let pk = PublicKey::new_dummy();
    let bootstrap_nodes = bootstap_peers("./data/starting_nodes.json");
    let mut network = P2PNetwork::new(7070, bootstrap_nodes);
    parse_args(&args, &mut network).unwrap();

    // TODO: Read from file
    let chain = Blockchain::new();
    let state = AppState::new(pk.clone(), network.listen_addr, chain);
    network.start(state.clone()).await?;

    let network_arc = Arc::new(network);
    let cli = Cli::new(network_arc.clone(), state.clone());

    cli.run().await?;
    Ok(())
}
