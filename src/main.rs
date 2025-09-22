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

mod block;
mod blockchain;
mod clargs;
mod cli;
mod errors;
mod keys;
mod logging;
mod networking;
mod peers;
mod types;
mod protocol;

static HISTORY_FOLDER: &'static str = "./data/.history";
static SHELL_HISTORY_LOC: &'static str = "./data/.history/shell.txt";

fn generate_files_if_needed() -> Result<()> {
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    generate_files_if_needed().unwrap();
    let args: Vec<String> = std::env::args().collect();

    let pk = PublicKey::new_dummy();
    let bootstrap_nodes = bootstap_peers("./data/starting_nodes.json");
    let mut network = P2PNetwork::new(7070, pk.clone(), bootstrap_nodes);
    parse_args(&args, &mut network).unwrap();

    // TODO: Read from file
    let chain = Blockchain::new();
    let state = AppState::new(pk.clone(), chain);
    network.start(state).await?;

    let network_arc = Arc::new(network);
    let cli = Cli::new(network_arc.clone());

    cli.run().await?;
    Ok(())
}
