use std::{io::{self, Write}, process::exit, sync::Arc};

use anyhow::Result;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};

use crate::{log_error, log_warn, networking::{P2PNetwork, PeerMessage}, SHELL_HISTORY_LOC};

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkCommand {
    List,
    Ping,
    Quit,
    Help,
    LastBlock,
}

impl NetworkCommand {
    pub fn help() -> String {
        let mut help = String::new();
        help.push_str("list      - list all connected peers\n");
        help.push_str("ping      - ping all connected peers\n");
        help.push_str("quit      - exit the application\n");
        help.push_str("help      - show available commands\n");
        help.push_str("lastBlock - request the last block from peers\n");
        help
    }
}

#[derive(Debug)]
pub struct Cli {
    network: Arc<P2PNetwork>,
    command_tx: Sender<NetworkCommand>,
}

impl Cli {
    pub fn new(network: Arc<P2PNetwork>) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel(100);
        let network_clone = network.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                match command {
                    NetworkCommand::List => {
                        let peers = network_clone.peers.lock().await;
                        println!("Connected peers:");
                        for (addr, peer) in peers.iter() {
                            println!(" {} - connected: {}", peer.pk, addr);
                        }
                    },
                    NetworkCommand::Ping => {
                        if let Err(e) = network_clone.broadcast(PeerMessage::Ping).await {
                            eprintln!("Error broadcasting ping command: {e}");
                        }
                    },
                    NetworkCommand::Quit => {
                        // Clean up
                        exit(0);
                    },
                    NetworkCommand::Help => {
                       println!("{}", NetworkCommand::help());
                    }
                    NetworkCommand::LastBlock => {
                        if let Err(e) = network_clone.broadcast(PeerMessage::GetLastBlock).await {
                            eprintln!("Error broadcasting request fot last block: {e}");
                        }
                    }
                }
            }
        });
        Self {network, command_tx}
    }

    fn ask(question: &str, answer: &mut String) -> io::Result<()> {
        print!("{question} ");
        std::io::stdout().flush()?;

        std::io::stdin().read_line(answer)?;
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
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
                            self.command_tx.send(NetworkCommand::List).await?;
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
}
