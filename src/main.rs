use std::{
    fs::{self},
    io::{self, Write},
    process::exit,
};

use anyhow::Result;
use colored::Colorize;
use rustyline::DefaultEditor;

use crate::{blockchain::Blockchain, keys::PublicKey, networking::{AppState, P2PNetwork}, peers::bootstap_peers};

mod block;
mod blockchain;
mod peers;
mod logging;
mod keys;
mod types;
mod networking;
mod errors;

static DEBUG: bool = true;
static HISTORY_FOLDER: &'static str = "./data/.history";
static SHELL_HISTORY_LOC: &'static str = "./data/.history/shell.txt";

pub fn qa(question: &str, answer: &mut String) -> io::Result<()> {
    print!("{question} ");
    std::io::stdout().flush()?;

    std::io::stdin().read_line(answer)?;
    Ok(())
}

pub async fn terminal_shell(network: &mut P2PNetwork) -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    let _ = rl.load_history(SHELL_HISTORY_LOC);
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let command = line.trim().to_lowercase().to_string();
                let _ = rl.add_history_entry(command.as_str());
                match command.as_str() {
                    "" => (),
                    "exit" => {
                        let _ = rl.save_history(SHELL_HISTORY_LOC);
                        exit(0)
                    }
                    "ls" => {
                    }
                    "start" => {
                        let mut buffer = String::new();
                        qa("incoming port (default 7070)?", &mut buffer)?;
                        let port_str = buffer.trim();
                        let port = match port_str.parse::<u16>() {
                            Ok(x) => {
                                if x < 1024 {
                                    log_warn!("Root privilages needed for ports smaller than 1024");
                                }
                                x
                            }
                            Err(e) => {
                                if !port_str.is_empty() {
                                    log_error!("Unexpected port number: {e}");
                                }
                                7070
                            }
                        };
                        network.set_port(port);
                        let pk = PublicKey::from_string("ff00ee22")?;
                        let chain = Blockchain::new();
                        let state = AppState::new(pk, chain);
                        network.start(state).await?;
                    }
                    _ => println!("Unknown command: {}", command),
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                let _ = rl.save_history(SHELL_HISTORY_LOC);
                exit(0)
            }
            Err(_) => (),
        }
    }
}

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
async fn main() -> Result<(), ()> {
    env_logger::init();
    generate_files_if_needed().unwrap();
    let args: Vec<String> = std::env::args().collect();
    if DEBUG {
        println!("{}", "*** Debug mode is ON ***".yellow());
    }
    let pk = PublicKey::from_string("ff32fe3").unwrap();
    let bootstrap_nodes = bootstap_peers("./data/starting_nodes.json");
    let mut network = P2PNetwork::new(7070, pk, bootstrap_nodes);
    if args.len() < 2 {
        terminal_shell(&mut network).await.unwrap();
    }
    Ok(())
}
