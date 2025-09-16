use core::str;
use std::{
    collections::HashMap,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{TcpListener, TcpSocket, TcpStream},
    sync::{mpsc::{self, Receiver, Sender}, Mutex, RwLock},
};

use crate::{
    block::Block, blockchain::Blockchain, cli::NetworkCommand, errors::NetworkError, keys::PublicKey, log_error, log_info, log_warn, peers::{self, Peer}
};

pub type SharedPeers = Arc<Mutex<HashMap<PublicKey, Peer>>>;
pub type BootstapNodes = Vec<Peer>;
pub static CONNECTION_LIMIT: u8 = 8;

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
    Pong,
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
pub enum ClientCommand {
    Broadcast(PeerMessage),
}

#[derive(Debug)]
pub struct P2PNetwork {
    pub peers: SharedPeers,
    pub listen_addr: SocketAddr,
    pub bootstrap_nodes: Vec<Peer>,
    pub pk: PublicKey,
    pub _version: u16,
    client_tx: Option<Arc<Sender<ClientCommand>>>,
}

impl P2PNetwork {
    pub fn new(listen_port: u16, pk: PublicKey, bootstrap_nodes: BootstapNodes) -> Self {
        let listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", listen_port)).unwrap();
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            listen_addr,
            bootstrap_nodes,
            pk,
            _version: 0,
            client_tx: None,
        }
    }

    pub fn set_port(&mut self, port: u16) {
        self.listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", port)).unwrap();
    }

    pub async fn start(&mut self, state: Arc<AppState>) -> Result<()> {
        let (client_tx, client_rx) = mpsc::channel(100);
        self.client_tx = Some(Arc::new(client_tx)); 

        let listener = TcpListener::bind(self.listen_addr).await?;
        log_info!("P2P node listening on {}", self.listen_addr);

        let peers = self.peers.clone();
        self.connect_to_bootstrap_nodes(peers).await?;

        let peers = self.peers.clone();
        let hs = HandShake::new(self.pk.clone(), self.listen_addr);
        tokio::spawn(async move {
            if let Err(e) = Self::accept_connections(state, listener, peers, hs).await {
                log_error!("Error accepting connection: {e}");
            }
        });

        let peers = self.peers.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::start_client(client_rx, peers).await {
                log_error!("Error connecting to peers: {e}");
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
                            }
                            Ok(None) => (),
                            Err(e) => {
                                log_error!("Error in handeling the connection: {e}");
                            }
                        }
                    });
                }
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
        loop {
            let n = reader.read_line(&mut data).await?;
            if n == 0 { break; }
            let incomming_msg: PeerMessage = serde_json::from_str(&data.trim())
                .context("parsing message from remote")
                .unwrap();
            match incomming_msg {
                PeerMessage::Handshake(hs) => {
                    log_info!("Handshake recived from {addr}");
                    let s = serde_json::to_string(&PeerMessage::Handshake(handshake))?;
                    writer.write_all(s.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                    return Ok(Some(hs.into()));
                }
                PeerMessage::Ping => {
                    log_info!("Ping recived from {addr}");
                    let s = serde_json::to_string(&PeerMessage::Pong)?;
                    writer.write_all(s.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                },
                PeerMessage::Pong => {
                    log_info!("Pong recived from {addr}");
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
                }
                PeerMessage::Error(e) => {
                    log_warn!("Error recived from {addr}: {e}");
                }
                _ => (),
            }
            data.clear();
        }

        Ok(None)
    }

    async fn send_handshake(
        stream: TcpStream,
        addr: SocketAddr,
        my_pk: PublicKey,
        my_addr: SocketAddr,
    ) -> Result<Peer> {
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
        let incomming_msg =
            serde_json::from_str(data.trim()).context("parsing handshake from remote")?;

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
                                match Self::send_handshake(stream, node.addr, my_pk, my_addr).await
                                {
                                    Ok(peer) => {
                                        peers.lock().await.insert(peer.pk.clone(), peer);
                                    }
                                    Err(e) => {
                                        log_warn!(
                                            "Error with outgoinh connection to {}: {:?}",
                                            node.addr,
                                            e
                                        );
                                    }
                                }
                                break;
                            }
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

    async fn start_client(mut rx: Receiver<ClientCommand>, peers: SharedPeers) -> Result<()> {
        let peers = peers.clone();
        let mut connections: HashMap<PublicKey, Arc<Mutex<BufWriter<TcpStream>>>> = HashMap::new();            

        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::Broadcast(message) => {
                    for (pk, peer) in peers.lock().await.iter() {
                        if !connections.contains_key(pk) {
                            match Self::connect_to_server(peer.addr).await {
                                Ok(w_buf) => {
                                    connections.insert(pk.clone(), Arc::new(Mutex::new(w_buf)));
                                }
                                Err(e) => {
                                    log_warn!("Failed to connect to {pk:?}@{}: {e}", peer.addr);
                                }
                            }
                        }
                    }
                    let s = serde_json::to_string(&message)?;
                    let msg = format!("{s}\n");

                    let mut broken = Vec::new();
                    for (pk, peer) in &connections {
                        let mut gaurd = peer.lock().await;
                        if let Err(e) = gaurd.write_all(msg.as_bytes()).await {
                            log_warn!("Failed to write to {pk:?}: {e}");
                            broken.push(pk.clone());
                            continue;
                        }
                        if let Err(e) = gaurd.flush().await {
                           log_warn!("Failed to flush to {pk:?}: {e}");
                           broken.push(pk.clone());
                        }
                    }
                    for pk in broken {
                        connections.remove(&pk);
                    }
                }
            }
        }
        log_warn!("client task ended: channel closed");
        Ok(())
    }

    pub async fn broadcast(&self, message: PeerMessage) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::Broadcast(message)).await?;
        Ok(())
    }

    async fn connect_to_server(addr: SocketAddr) -> Result<BufWriter<TcpStream>> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        Ok(BufWriter::with_capacity(8192, stream))
    }
}
