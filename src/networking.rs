use core::str;
use std::{collections::HashMap, net::SocketAddr, str::FromStr, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter}, net::{TcpListener, TcpStream}, sync::{Mutex, RwLock}};

use crate::{block::Block, blockchain::Blockchain, errors::NetworkError, keys::PublicKey, log_error, log_info, log_warn, peers::Peer};

pub type SharedPeers = Arc<Mutex<HashMap<PublicKey, Peer>>>;
pub type BootstapNodes = Vec<Peer>;
pub static CONNECTION_LIMIT : u8 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandShake {
    pub pk: PublicKey,
    pub timestamp: u64,
    pub addr: SocketAddr,
}

impl HandShake {
    pub fn new(pk: PublicKey, addr: SocketAddr) -> Self {
        Self {
            pk,
            addr,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

impl Into<Peer> for HandShake {
    fn into(self) -> Peer {
        Peer {
            pk: self.pk,
            addr: self.addr,
            is_connected: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    Handshake(HandShake),
    GetBlockByHash(String),
    GetBlockById(String),
    GetLastBlock,
    Block(Block),
    Error(NetworkError),
    Ping,
}

pub struct AppState {
    pub pk: RwLock<PublicKey>,
    pub chain: RwLock<Blockchain>,
}
impl AppState {
    pub fn new(pk: PublicKey, chain: Blockchain) -> Arc<Self> {
        Arc::new(Self {
            pk: RwLock::new(pk),
            chain: RwLock::new(chain),
        })
    }
}

#[derive(Debug)]
pub struct P2PNetwork {
    pub peers: SharedPeers,
    pub listen_addr: SocketAddr,
    pub bootstrap_nodes: Vec<Peer>,
    pub pk: PublicKey,
    pub version: u16,
}

impl P2PNetwork {
    pub fn new(listen_port: u16, pk: PublicKey, bootstrap_nodes: BootstapNodes) -> Self {
        let listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", listen_port)).unwrap();
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            listen_addr,
            bootstrap_nodes,
            pk,
            version: 0,
        }
    }
    
    pub fn set_port(&mut self, port: u16) {
        self.listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", port)).unwrap();
    }

    pub async fn start(&self, state: Arc<AppState>) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        log_info!("P2P node listening on {}",self.listen_addr);

        let peers = self.peers.clone();
        self.connect_to_bootstrap_nodes(peers).await?;

        let peers = self.peers.clone();
        let hs = HandShake::new(self.pk.clone(), self.listen_addr);
        tokio::spawn(async move {
            if let Err(e) = Self::accept_connections(state, listener, peers, hs).await {
                log_error!("Error accepting connection: {e}");
            } 
        });

        Ok(())
    }

    async fn accept_connections(
        state: Arc<AppState>,
        listener: TcpListener,
        peers: SharedPeers,
        handshake: HandShake,
    ) -> Result<()> {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    log_info!("New connection from {addr}");

                    let peers = peers.clone();
                    let hs = handshake.clone();
                    let state = state.clone();
                    tokio::spawn(async move {
                        match Self::handle_incomming(state, stream, addr.clone(), hs).await {
                            Ok(Some(peer)) => {
                                peers.lock().await.insert(peer.pk.clone(), peer);
                            },
                            Ok(None) => (),
                            Err(e) => {
                                log_error!("Error in handeling the connection: {e}");
                            }
                        }
                    });
                },
                Err(e) => {
                    log_error!("Error reciving the connection: {e}");
                }
            }
        }
    }

    async fn handle_incomming(
        state: Arc<AppState>,
        stream: TcpStream,
        addr: SocketAddr,
        handshake: HandShake,
    ) -> Result<Option<Peer>> {
        let (reader, mut writer) = stream.into_split();
        let mut writer = BufWriter::new(&mut writer);
        let mut reader = BufReader::new(reader);

        let mut data: String = String::new();
        let n = reader.read_line(&mut data).await?;
        if n == 0 {
            anyhow::bail!("connection closed during transaction");
        }
        let incomming_msg: PeerMessage = serde_json::from_str(&data.trim()).context("parsing message from remote").unwrap();
        match incomming_msg {
            PeerMessage::Handshake(hs) => {
                log_info!("Handshake recived from {addr}");
                let s = serde_json::to_string(&PeerMessage::Handshake(handshake))?;
                writer.write_all(s.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                return Ok(Some(hs.into()));
            },
            PeerMessage::GetLastBlock => {
               let chain = state.chain.read().await;
               if let Some(block) = (*chain).last_block() {
                   let s = serde_json::to_string(&PeerMessage::Block(block))?;
                   writer.write_all(s.as_bytes()).await?;
                   writer.write_all(b"\n").await?;
                   writer.flush().await?;
               } else {
                   let s = serde_json::to_string(&PeerMessage::Error(NetworkError::Block404))?;
                   writer.write_all(s.as_bytes()).await?;
                   writer.write_all(b"\n").await?;
                   writer.flush().await?;
               }
            },
            PeerMessage::Error(e) => {
                log_warn!("Error recived from {addr}: {e}");
            }
            _ => (),
        }

        Ok(None) 
    }
    
    async fn send_handshake(stream: TcpStream, addr: SocketAddr, my_pk: PublicKey, my_addr: SocketAddr) -> Result<Peer> {
        let (reader, mut writer) = stream.into_split();
        let mut writer = BufWriter::new(&mut writer);
        let mut reader = BufReader::new(reader);

        let handshake = PeerMessage::Handshake(HandShake::new(my_pk, my_addr));
        let s = serde_json::to_string(&handshake)?;
        writer.write_all(s.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        log_info!("sending handshake to {}", addr);

        let mut data = String::new();
        let n = reader.read_line(&mut data).await?;
        if n == 0 {
            anyhow::bail!("connection closed during handshake");
        }
        let incomming_msg = serde_json::from_str(data.trim()).context("parsing handshake from remote")?;

        if let PeerMessage::Handshake(handshake) = incomming_msg {
            Ok(handshake.into())
        } else {
            anyhow::bail!("no handshake recived from peer");
        }
    }
    
    async fn connect_to_bootstrap_nodes(&self, peers: SharedPeers) -> Result<()> {
        // Note: Functions with multithreading can only have owned refrences
        // of arguments past using self ref to the original method so cloning
        // is nessesery.
        let my_pk = self.pk.clone();
        let my_addr = self.listen_addr.clone();
        let nodes = self.bootstrap_nodes.clone();
        for node in nodes {
            if node.addr != self.listen_addr {
                let peers = peers.clone();
                let my_pk = my_pk.clone();
                tokio::spawn(async move {
                    let mut attempt = 0;
                    loop {
                        if attempt > 3 {
                            break;
                        }
                        match TcpStream::connect(node.addr).await {
                            Ok(stream) => {
                                match Self::send_handshake(stream, node.addr, my_pk, my_addr).await {
                                    Ok(peer) => {
                                        peers.lock().await.insert(peer.pk.clone(), peer);
                                    },
                                    Err(e) => {
                                        log_warn!("Error with outgoinh connection to {}: {:?}", node.addr, e);
                                    }
                                }
                                break;
                            },
                            Err(_) => {
                                attempt += 1;
                            }
                        }
                    }
                });
            }
        }
        Ok(())
    }

    pub async fn broadcast(&self, message: PeerMessage) -> Result<()> {
        Ok(())
    }

}
