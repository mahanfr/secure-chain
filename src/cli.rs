use std::{
    io::{self, Write}, process::exit, sync::Arc
};

use anyhow::Result;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, Sender};

use crate::{
    networking::{AppState, P2PNetwork, PeerMessage}, SHELL_HISTORY_LOC
};

#[derive(Debug, Serialize, Deserialize)]
pub enum NetworkCommand {
    List,
    Ping(u64),
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
    state: Arc<AppState>,
    command_tx: Sender<NetworkCommand>,
}

impl Cli {
    pub fn new(network: Arc<P2PNetwork>, state: Arc<AppState>) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel(100);
        let network_clone = network.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                match command {
                    NetworkCommand::List => {
                        let peers = state_clone.peers.lock().await;
                        println!("Connected peers:");
                        for (addr, peer) in peers.iter() {
                            println!(" {} - connected: {}", peer.pk, addr);
                        }
                    }
                    NetworkCommand::Ping(val) => {
                        if val == 0 {
                            if let Err(e) = network_clone.broadcast(PeerMessage::Ping).await {
                                eprintln!("Error broadcasting ping command: {e}");
                            }
                        } else {
                            if let Err(e) = network_clone.ping(val).await {
                                eprintln!("Error pinging the user {val} command: {e}");
                            }
                        }
                    }
                    NetworkCommand::Quit => {
                        // Clean up
                        exit(0);
                    }
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
        Self {
            network,
            state,
            command_tx,
        }
    }

    fn _ask(question: &str, answer: &mut String) -> io::Result<()> {
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
                    let c_line = line.trim().to_lowercase().to_string();
                    let c_args: Vec<&str> = c_line.split_whitespace().collect();
                    let _ = rl.add_history_entry(c_line.as_str());
                    if c_args.is_empty() {
                        continue;
                    }
                    match c_args[0] {
                        "" => (),
                        "exit" => {
                            let _ = rl.save_history(SHELL_HISTORY_LOC);
                            exit(0)
                        }
                        "ls" => {
                            self.command_tx.send(NetworkCommand::List).await?;
                        }
                        "ping" => {
                            if let Some(who) = c_args.iter().nth(1) {
                                match *who {
                                    "all" => {
                                        self.command_tx.send(NetworkCommand::Ping(0)).await?;
                                    }
                                    s => {
                                        if s.len() < 5 {
                                            println!("please provide at least the first 5 numbers of the peer id");
                                            continue;
                                        } else {
                                            let peers = self.state.peers.lock().await;
                                           let peer_ids: Vec<&u64> = peers.keys()
                                               .filter(|k| k.to_string().starts_with(s)).collect();
                                            for peer_id in peer_ids {
                                               self.command_tx.send(NetworkCommand::Ping(*peer_id)).await?;
                                            }
                                        }
                                    }
                                }
                            } else {
                                println!("Please provide: <all> to ping all connected peers or at least the first 5 character of their peer id");
                            }
                        }
                        _ => println!("Unknown command: {}", c_args[0]),
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
