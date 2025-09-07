use std::{collections::HashMap, fs::{self, create_dir}, io::{self, Write}, net::SocketAddr, process::exit, str::FromStr, sync::Arc};

use anyhow::{Context, Result};
use colored::Colorize;
use rustyline::DefaultEditor;
use serde::{Deserialize, Serialize};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter}, net::{TcpListener, TcpStream}, sync::Mutex};
use uuid::Uuid;

use crate::bootstrap::read_nodes_list;

mod blockchain;
mod types;
mod block;
mod bootstrap;
mod p2p;
mod logging;

static DEBUG : bool = true;

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    Handshake { id: String },
    Ping,
    Pong,
}

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

    let peers: SharedPeers = Arc::new(Mutex::new(HashMap::new()));
    let node_id = Uuid::new_v4();
    let listener = TcpListener::bind(&listen_addr)
        .await
        .with_context(|| format!("failed to bind to {}", listen_addr))?;

    {
        let peers = peers.clone();
        let node_id = node_id.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let peers = peers.clone();
                        let node_id = node_id.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, addr, node_id, peers).await {
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
        let peers = peers.clone();
        let node_id = node_id.clone();
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
                        if let Err(e) = handle_outgoing(stream, node_addr, node_id.clone(), peers.clone()).await {
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

pub async fn handle_connection(stream: TcpStream, addr: SocketAddr, master_id: Uuid, peers: SharedPeers) -> Result<()> {
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
            let ours = Message::Handshake { id: master_id.to_string() };
            let s = serde_json::to_string(&ours)?;
            writer.write_all(s.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;

            peers.lock().await.insert(id.clone(), addr);
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

pub async fn handle_outgoing(stream: TcpStream, addr: SocketAddr, master_id: Uuid, peers: SharedPeers) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut writer = BufWriter::new(&mut writer);
    let mut reader = BufReader::new(reader);

    let ours = Message::Handshake { id: master_id.to_string() };
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
            peers.lock().await.insert(remote_id.clone(), addr);
        },
        _ => (),
    }
    Ok(())
}

pub async fn terminal_shell() -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    let history_loc = "./data/.history/shell.txt";
    let _ = rl.load_history(history_loc);
    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let command = line.trim().to_lowercase().to_string();
                let _ = rl.add_history_entry(command.as_str());
                match command.as_str() {
                    "" => (),
                    "exit" => {
                        let _ = rl.save_history(history_loc);
                        exit(0)
                    },
                    "ls" => { },
                    "start" => {
                        let mut buffer = String::new();
                        qa("incoming port (default 7070)?", &mut buffer)?;
                        let port_str = buffer.trim(); 
                        let port = match port_str.parse::<u16>() {
                            Ok(x) => {
                                if x < 1024 {
                                    println!("{}: Root privilages needed for ports smaller than 1024","warning".yellow());
                                }
                                x   
                            },
                            Err(e) => {
                                if !port_str.is_empty() {
                                    eprintln!("Unexpected port number: {e}");
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
                let _ = rl.save_history(history_loc);
                exit(0)
            },
            Err(_) => (),
        }
    }
}

fn generate_files_if_needed() -> Result<()> {
    match fs::create_dir("./data/.history") {
        Ok(()) => (),
        Err(e) => {
            if e.kind() != io::ErrorKind::AlreadyExists {
                panic!("cannot create folder: {}",e);
            }
        }
    }

    let history_loc = "./data/.history/shell.txt";
    let _ = fs::File::create(history_loc);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(),()> {
    env_logger::init();
    generate_files_if_needed().unwrap();
    let args: Vec<String> = std::env::args().collect();
    if DEBUG { println!("{}","*** Debug mode is ON ***".yellow()); }
    if args.len() < 2 {
        terminal_shell().await.unwrap();
    }
    Ok(())
}
