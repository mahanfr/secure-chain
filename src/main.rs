use std::{collections::HashMap, fs::{self}, io::{self, Write}, net::SocketAddr, process::exit, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use colored::Colorize;
use lazy_static::lazy_static;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter}, net::{TcpListener, TcpStream}, sync::Mutex};
use uuid::Uuid;

use crate::{block::Block, bootstrap::read_nodes_list};

mod blockchain;
mod types;
mod block;
mod bootstrap;
mod p2p;
mod logging;

static DEBUG : bool = true;
static HISTORY_FOLDER: &'static str = "./data/.history";
static SHELL_HISTORY_LOC: &'static str = "./data/.history/shell.txt";

lazy_static! {
    static ref ACTIVE_PEERS: Mutex<HashMap<String, Peer>> = Mutex::new(HashMap::new());
    static ref PUBLIC_KEY: Mutex<String> = Mutex::new(String::new()); // For now
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Peer {
    pub public_key: String,
    pub addr: SocketAddr,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Handshake { id: String },
    //ReqBlockById(String), // Block ID
    //ReqBlockByHash(String), // Block ID
    //Block(Block),
    Ping,
    Pong,
}

// Redundent for now i cant decide between global and local variable for Peers
type SharedPeers = Arc<Mutex<HashMap<String, SocketAddr>>>;

pub fn qa(question: &str, answer: &mut String) -> io::Result<()> {
    print!("{question} ");
    std::io::stdout().flush()?;

    std::io::stdin().read_line(answer)?;
    Ok(())
}

pub async fn run_server(port: u16) -> Result<()> {
    let listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{port}"))?;
    let trusted_nodes = read_nodes_list("data/starting_nodes.json");

    let listener = TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("failed to bind to {}", listen_addr))?;

    {
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, addr).await {
                                log_warn!("Connection {} error: {:?}", addr, e);
                            }
                        });
                    },
                    Err(e) => {
                        log_error!("Accept error: {:?}", e);
                    }
                }
            }
        });
    }

    for node in trusted_nodes {
        let Ok(node_addr): Result<SocketAddr, _> = node.addr().parse() else {
            continue;
        };
        if node_addr == listen_addr {
            continue;
        }
        tokio::spawn(async move {
            let mut attempt = 0;
            loop {
                if attempt > 3 { break; }
                match TcpStream::connect(node_addr).await {
                    Ok(stream) => {
                        if let Err(e) = handle_outgoing(stream, node_addr).await {
                            log_warn!("Error with outgoing connection to {}: {:?}",node_addr, e);
                        }
                        break;
                    },
                    Err(_) => {
                        //eprintln!("Faild to connect to {}: {:?}", node_addr, e);
                        attempt += 1;
                    }
                }
            }
        });
    }

    Ok(())
}

pub async fn handle_connection(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut writer = BufWriter::new(&mut writer);
    let mut reader = BufReader::new(reader);

    let mut data = String::new();
    let n = reader.read_line(&mut data).await?;
    if n == 0 {
        anyhow::bail!("connection closed during handshake");
    }
    let remote_msg: Message = serde_json::from_str(data.trim())
        .context("parsing handshake from remote")?;
    match remote_msg {
        Message::Handshake { id } => {
            log_success!("Handshake recived from {} -> id {}", addr, id);
            let ours = Message::Handshake { id: PUBLIC_KEY.lock().await.to_string() };
            let s = serde_json::to_string(&ours)?;
            writer.write_all(s.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
            ACTIVE_PEERS.lock().await.insert(id.clone(), Peer {public_key: id.clone(), addr: addr});
        },
        Message::Ping => {
            let reply = serde_json::to_string(&Message::Pong)?;
            writer.write_all(reply.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        },
        Message::Pong => {
            log_info!("Pong from {}", addr);
        },
    }
    Ok(())
}

pub async fn handle_outgoing(stream: TcpStream, addr: SocketAddr) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut writer = BufWriter::new(&mut writer);
    let mut reader = BufReader::new(reader);

    let ours = Message::Handshake { id: PUBLIC_KEY.lock().await.to_string() };
    let s = serde_json::to_string(&ours)?;
    writer.write_all(s.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    log_info!("sending handshake to {}", addr);

    let mut data = String::new();
    let n = reader.read_line(&mut data).await?;
    if n == 0 {
        anyhow::bail!("connection closed during handshake");
    }
    let remote_msg: Message = serde_json::from_str(data.trim())
        .context("parsing handshake from remote")?;

    match remote_msg {
        Message::Handshake { id: remote_id } => {
            log_success!("resived handshake");
            ACTIVE_PEERS.lock().await.insert(remote_id.clone(), Peer {public_key: remote_id.clone(), addr: addr});
        },
        _ => (),
    }
    Ok(())
}

pub async fn terminal_shell() -> Result<()> {
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
                    },
                    "ls" => {
                        for peer in ACTIVE_PEERS.lock().await.values() {
                            println!("Peer {:?}", peer);
                        }
                    },
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
                            },
                            Err(e) => {
                                if !port_str.is_empty() {
                                    log_error!("Unexpected port number: {e}");
                                }
                                7070
                            },
                        };
                        run_server(port).await?;
                    },
                    _ => println!("Unknown command: {}",command),
                }
            },
            Err(rustyline::error::ReadlineError::Interrupted) => {
                let _ = rl.save_history(SHELL_HISTORY_LOC);
                exit(0)
            },
            Err(_) => (),
        }
    }
}

fn generate_files_if_needed() -> Result<()> {
    match fs::create_dir(HISTORY_FOLDER) {
        Ok(()) => {
            let _ = fs::File::create(SHELL_HISTORY_LOC);
        },
        Err(e) => {
            if e.kind() != io::ErrorKind::AlreadyExists {
                panic!("cannot create folder: {}",e);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(),()> {
    env_logger::init();
    generate_files_if_needed().unwrap();
    // TODO: Read/Create Public key (not sure how)
    // TODO: This is bad. should be context insted of global variable (works for now)
    *PUBLIC_KEY.lock().await = Uuid::new_v4().to_string();
    let args: Vec<String> = std::env::args().collect();
    if DEBUG { println!("{}","*** Debug mode is ON ***".yellow()); }
    if args.len() < 2 {
        terminal_shell().await.unwrap();
    }
    Ok(())
}
