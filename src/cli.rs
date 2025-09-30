use std::{process::exit, sync::Arc};

use anyhow::Result;
use rustyline::DefaultEditor;
use tokio::sync::mpsc::{self, Sender};

use crate::{
    SHELL_HISTORY_LOC,
    networking::{AppState, P2PNetwork, PeerMessage},
    peers::Peer,
};

#[derive(Debug)]
enum CliCommand {
    List,
    Ping(u64),
    ConnectTo(Peer),
    StartServer,
    StopServer,
}

impl CliCommand {
    pub fn help() -> String {
        let mut help = String::new();
        help.push_str("list              - list all connected peers\n");
        help.push_str("ping     all|<id> - ping connected peers\n");
        help.push_str("server start|stop - start or stop incoming server\n");
        help.push_str("quit              - exit the application\n");
        help.push_str("help              - show available commands\n");
        help.push_str("lastBlock         - request the last block from peers\n");
        help
    }
}

#[derive(Debug)]
pub struct Cli {
    state: Arc<AppState>,
    command_tx: Sender<CliCommand>,
}

impl Cli {
    pub fn new(network: Arc<P2PNetwork>, state: Arc<AppState>) -> Self {
        let (command_tx, mut command_rx) = mpsc::channel(100);
        let network_clone = network.clone();
        let state_clone = state.clone();
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                match command {
                    CliCommand::StartServer => {
                        if let Err(e) = network_clone.start_server(state_clone.clone()).await {
                            println!("Error starting server: {e}");
                        }
                    }
                    CliCommand::StopServer => {
                        if let Err(e) = network_clone.stop_server().await {
                            println!("Error stoping the server: {e}");
                        }
                    }
                    CliCommand::List => {
                        let peers = state_clone.peers.lock().await;
                        println!("Connected peers:");
                        for (addr, peer) in peers.iter() {
                            println!(" {} - connected: {}", peer.pk, addr);
                        }
                    }
                    CliCommand::Ping(val) => {
                        if val == 0 {
                            if let Err(e) = network_clone.broadcast(PeerMessage::Ping).await {
                                eprintln!("Error broadcasting ping command: {e}");
                            }
                        } else if let Err(e) = network_clone.ping(val).await {
                            eprintln!("Error pinging the user {val} command: {e}");
                        }
                    }
                    CliCommand::ConnectTo(peer) => {
                        if let Err(e) = network_clone.connect_to_peer(peer).await {
                            eprintln!("Error broadcasting request fot last block: {e}");
                        }
                    }
                }
            }
        });
        Self { state, command_tx }
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
                            self.command_tx.send(CliCommand::List).await?;
                        }
                        "help" => {
                            println!("{}", CliCommand::help());
                        }
                        "server" => {
                            if let Some(sub_cmd) = c_args.get(1) {
                                match *sub_cmd {
                                    "start" => {
                                        self.command_tx.send(CliCommand::StartServer).await?;
                                    }
                                    "stop" => {
                                        self.command_tx.send(CliCommand::StopServer).await?;
                                    }
                                    _ => {
                                        println!("Unknown sub command");
                                    }
                                }
                            } else {
                                println!("Please provide a sub command");
                            }
                        }
                        "connect" => {
                            if let Some(peer_uri) = c_args.get(1) {
                                match Peer::from_string(peer_uri.to_string()) {
                                    Ok(peer) => {
                                        self.command_tx.send(CliCommand::ConnectTo(peer)).await?;
                                    }
                                    Err(e) => {
                                        println!("Error parsing peer uri: {e}")
                                    }
                                };
                            } else {
                                println!("Please provide a peer uri");
                            }
                        }
                        "ping" => {
                            if let Some(who) = c_args.get(1) {
                                match *who {
                                    "all" => {
                                        self.command_tx.send(CliCommand::Ping(0)).await?;
                                    }
                                    s => {
                                        if s.len() < 5 {
                                            println!(
                                                "please provide at least the first 5 numbers of the peer id"
                                            );
                                            continue;
                                        } else {
                                            let peers = self.state.peers.lock().await;
                                            let peer_ids: Vec<&u64> = peers
                                                .keys()
                                                .filter(|k| k.to_string().starts_with(s))
                                                .collect();
                                            for peer_id in peer_ids {
                                                self.command_tx
                                                    .send(CliCommand::Ping(*peer_id))
                                                    .await?;
                                            }
                                        }
                                    }
                                }
                            } else {
                                println!(
                                    "Please provide: <all> to ping all connected peers or at least the first 5 character of their peer id"
                                );
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
