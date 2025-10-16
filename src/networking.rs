use std::{
    collections::HashMap,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use bincode::{config, Decode, Encode};
use rand::Rng;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{
        TcpListener, TcpStream,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{
        Mutex, RwLock,
        mpsc::{self, Receiver, Sender},
        watch,
    },
    task::JoinHandle,
};

use crate::{
    block::Block, blockchain::Blockchain, errors::NetworkError, keys::PublicKey, peers::{Peer, PeerOptions}, protocol::{
        header::{ContentType, P2ProtHeader}, P2Protocol
    }
};

pub type PeerId = u128;
pub type SharedPeers = Arc<Mutex<HashMap<PeerId, Peer>>>;
pub type BootstapNodes = Vec<Peer>;

#[derive(Debug, Clone, Encode, Decode)]
pub struct HandShake {
    pub pk: PublicKey,
    pub addr: SocketAddr,
}

impl HandShake {
    pub fn new(pk: PublicKey, addr: SocketAddr) -> Self {
        Self { pk, addr }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct HandShakeAck {
    pub pk: PublicKey,
    pub timestamp: u128,
    pub addr: SocketAddr,
}

impl HandShakeAck {
    pub fn new(pk: PublicKey, addr: SocketAddr) -> Self {
        Self {
            pk,
            addr,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        }
    }
}

#[derive(Debug, Clone, Encode, Decode)]
#[repr(u16)]
pub enum PeerMessage {
    Gossip(Vec<Peer>),
    Handshake(HandShake),
    HandshakeAck(HandShakeAck),
    GetBlockByHash(String),
    GetBlockById(String),
    GetLastBlock,
    Block(Block),
    Error(NetworkError),
    Ping,
    Pong,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct AppState {
    pub pk: PublicKey,
    pub addr: SocketAddr,
    pub chain: RwLock<Blockchain>,
    pub peers: SharedPeers,
}
impl AppState {
    pub fn new(pk: PublicKey, addr: SocketAddr, chain: Blockchain) -> Arc<Self> {
        Arc::new(Self {
            pk,
            addr,
            chain: RwLock::new(chain),
            peers: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

#[derive(Debug)]
pub enum ClientCommand {
    Broadcast(PeerMessage),
    Ping(PeerId),
    Pong(PeerId),
    SendHandShakeAck(SocketAddr, HandShake),
    ConnectToPeer(Peer),
    NewServerConnection(SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>),
    NewClientConnection(SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>),
    EstablishedPeerConnection(SocketAddr, Peer),
}

#[derive(Debug)]
pub struct ServerState {
    server_handle: Option<JoinHandle<()>>,
    stop_tx: Option<watch::Sender<bool>>,
}

#[derive(Debug)]
pub struct P2PNetwork {
    pub listen_addr: SocketAddr,
    pub bootstrap_nodes: Vec<Peer>,
    client_tx: Option<Arc<Sender<ClientCommand>>>,
    server_state: Arc<Mutex<ServerState>>,
}

impl P2PNetwork {
    pub fn new(listen_port: u16, bootstrap_nodes: BootstapNodes) -> Self {
        let listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", listen_port)).unwrap();
        Self {
            listen_addr,
            bootstrap_nodes,
            client_tx: None,
            server_state: Arc::new(Mutex::new(ServerState {
                server_handle: None,
                stop_tx: None,
            })),
        }
    }

    pub fn set_port(&mut self, port: u16) {
        self.listen_addr = SocketAddr::from_str(&format!("0.0.0.0:{}", port)).unwrap();
    }

    pub async fn start_server(&self, state: Arc<AppState>) -> Result<()> {
        let mut s_state = self.server_state.lock().await;
        if s_state.server_handle.is_some() {
            anyhow::bail!("server is already running!");
        }
        let listener = TcpListener::bind(self.listen_addr).await?;
        tracing::info!(addr = %self.listen_addr, "P2P node listening");

        // stop channel
        let (stop_tx, stop_rx) = watch::channel(false);
        s_state.stop_tx = Some(stop_tx);

        let state_cloned = state.clone();
        let Some(tx_cloned) = self.client_tx.clone() else {
            anyhow::bail!("client communication channel is not ready, please try again");
        };
        let server_handle = tokio::spawn(async move {
            if let Err(e) =
                Self::accept_connections(state_cloned, tx_cloned, listener, stop_rx).await
            {
                tracing::error!(error = ?e, "Error Accepting Connection");
            }
        });
        s_state.server_handle = Some(server_handle);
        Ok(())
    }

    pub async fn stop_server(&self) -> Result<()> {
        let mut s_state = self.server_state.lock().await;
        if let Some(stop_tx) = &s_state.stop_tx {
            stop_tx.send(true)?;
        }
        if let Some(handle) = s_state.server_handle.take() {
            let _ = handle.await;
        }
        tracing::info!("server service is stopped");
        Ok(())
    }

    pub async fn start(&mut self, state: Arc<AppState>) -> Result<()> {
        let (client_tx, client_rx) = mpsc::channel(100);
        let tx = Arc::new(client_tx);
        self.client_tx = Some(tx.clone());

        if let Err(e) = self.start_server(state.clone()).await {
            tracing::error!(error = ?e, "Error Starting The Server");
        }

        let state_cloned = state.clone();
        let tx_cloned = tx.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::start_client(state_cloned, tx_cloned, client_rx).await {
                tracing::error!(error = ?e, "Error Connecting to Peers");
            }
        });

        let state_cloned = state.clone();
        let tx_cloned = tx.clone();
        if let Err(e) = self
            .connect_to_bootstrap_nodes(state_cloned, tx_cloned)
            .await
        {
            tracing::error!(error = ?e, "Error Connecting to bootstrap nodes");
        }

        Ok(())
    }

    async fn accept_connections(
        state: Arc<AppState>,
        client_tx: Arc<Sender<ClientCommand>>,
        listener: TcpListener,
        mut stop_rx: watch::Receiver<bool>,
    ) -> Result<()> {
        loop {
            tokio::select! {
                biased;
                _ = stop_rx.changed() => {
                    if *stop_rx.borrow() {
                        tracing::info!("server stopping gracefully...");
                        break;
                    }
                }

                res = listener.accept() => {
                    match res {
                        Ok((stream, addr)) => {
                            tracing::info!(addr = %addr, "New connection");

                            let (r, w) = stream.into_split();
                            let reader = BufReader::new(r);
                            let writer = BufWriter::new(w);
                            client_tx.send(ClientCommand::NewServerConnection(addr, Arc::new(Mutex::new(writer)))).await?;

                            let state = state.clone();
                            let client_tx = client_tx.clone();
                            tokio::spawn(async move {
                                match Self::handle_incomming(state, reader, client_tx, addr).await {
                                    Ok(_) => {
                                        tracing::warn!(addr = %addr, "Connection Closed");
                                    },
                                    Err(e) => {
                                        tracing::error!(addr = %addr, error = ?e ,"Error in handeling the connection");
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = ?e ,"Error reciving the connection");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_incomming(
        state: Arc<AppState>,
        mut reader: BufReader<OwnedReadHalf>,
        sender: Arc<Sender<ClientCommand>>,
        addr: SocketAddr,
    ) -> Result<()> {
        let mut data: String = String::new();
        let mut header_buf = vec![0u8; 40];
        loop {
            if let Err(_) = reader.read_exact(&mut header_buf).await {
                tracing::warn!(addr = %addr, "Connection closed");
                break;
            }
            let header = match P2ProtHeader::from_bytes(&header_buf) {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!(error = ?e ,"Error parsing the header");
                    break;
                }
            };
            // DO SOME CHECKS ON HEADER
            if header.content_type != ContentType::HandShake && header.session_id == 0 {
                tracing::error!("Incomming message had no valid session id");
                break;
            }
            // DO SOME CHECKS ON HEADER
            let mut payload = vec![0u8; header.size];
            if let Err(e) = reader.read_exact(&mut payload).await {
                tracing::error!(error = ?e ,"Error reading the message");
                break;
            }
            let (incoming_msg, _): (PeerMessage, _) =
                match bincode::decode_from_slice(&payload, config::standard()) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!(error = ?e ,"Error parsing the message");
                        break;
                    }
                };
            match incoming_msg {
                PeerMessage::Gossip(goss) => {
                }
                PeerMessage::Handshake(recv_hs) => {
                    tracing::info!(addr = %addr, "Handshake recived");
                    if let Err(e) = sender
                        .send(ClientCommand::SendHandShakeAck(addr, recv_hs))
                        .await
                    {
                        tracing::error!(error = ?e ,"Unable to communicate with client service");
                        break;
                    }
                }
                PeerMessage::HandshakeAck(recv_hs) => {
                    let peer = Peer::new(recv_hs.addr, recv_hs.pk.clone());
                    if let Err(e) = sender
                        .send(ClientCommand::EstablishedPeerConnection(
                            addr,
                            peer,
                        ))
                        .await
                    {
                        tracing::error!(error = ?e ,"Unable to communicate with client service");
                        break;
                    }
                }
                PeerMessage::Ping => {
                    tracing::info!(addr = %addr, "Ping Recived");
                    if let Err(e) = sender.send(ClientCommand::Pong(header.session_id)).await {
                        tracing::error!(error = ?e ,"Unable to communicate with client service");
                        break;
                    }
                }
                PeerMessage::Pong => {
                    tracing::info!(addr = %addr, "Pong Recived");
                }
                PeerMessage::Error(e) => {
                    tracing::error!(addr = %addr, error = ?e ,"Error recived");
                }
                _ => (),
            }
            data.clear();
        }

        Ok(())
    }

    pub async fn connect_to_bootstrap_nodes(
        &self,
        state: Arc<AppState>,
        client_tx: Arc<Sender<ClientCommand>>,
    ) -> Result<()> {
        let nodes = self.bootstrap_nodes.clone();
        for node in nodes {
            if node.addr != self.listen_addr {
                let state = state.clone();
                let client_tx = client_tx.clone();
                tokio::spawn(async move {
                    let mut attempt = 0;
                    loop {
                        if attempt > 3 {
                            break;
                        }
                        match Self::connect_to_server(state.clone(), client_tx.clone(), node.addr)
                            .await
                        {
                            Ok(w_buf) => {
                                let _ = client_tx
                                    .send(ClientCommand::NewClientConnection(
                                        node.addr,
                                        Arc::new(Mutex::new(w_buf)),
                                    ))
                                    .await;
                                break;
                            }
                            Err(e) => {
                                tracing::warn!(addr = %node.addr ,error = ?e,"Error connecting to bootstrap peer");
                                attempt += 1;
                            }
                        }
                    }
                });
            }
        }
        Ok(())
    }

    async fn send(writer: Arc<Mutex<BufWriter<OwnedWriteHalf>>>, prot: &P2Protocol) -> Result<()> {
        writer.lock().await.write_all(&prot.serialize()?).await?;
        writer.lock().await.flush().await?;
        Ok(())
    }

    async fn start_client(
        state: Arc<AppState>,
        client_tx: Arc<Sender<ClientCommand>>,
        mut rx: Receiver<ClientCommand>,
    ) -> Result<()> {
        let peers = state.peers.clone();
        let mut connections: HashMap<PeerId, Arc<Mutex<BufWriter<OwnedWriteHalf>>>> =
            HashMap::new();
        let mut queue_conn: HashMap<SocketAddr, Arc<Mutex<BufWriter<OwnedWriteHalf>>>> =
            HashMap::new();

        let tx = client_tx.clone();
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ClientCommand::NewServerConnection(addr, writer) => {
                    queue_conn.insert(addr, writer);
                }
                ClientCommand::NewClientConnection(addr, writer) => {
                    let handshake = HandShake::new(state.pk.clone(), state.addr);
                    let prot = P2Protocol::new(&PeerMessage::Handshake(handshake));
                    if let Err(e) = Self::send(writer.clone(), &prot).await {
                        tracing::error!(addr = %addr ,error = ?e,"Can not send message");
                    } else {
                        queue_conn.insert(addr, writer);
                    }
                }
                ClientCommand::EstablishedPeerConnection(addr, peer) => {
                    if let Ok(peer_id) = peer.derive_unique_id() {
                        if let Some(writer) = queue_conn.remove(&addr) {
                            peers.lock().await.insert(peer_id, peer);
                            connections.insert(peer_id, writer);
                        }
                    } else {
                        tracing::error!("Failed to drive peer id");
                    }
                }
                ClientCommand::SendHandShakeAck(addr, hs) => {
                    let handshake_ack = HandShakeAck::new(state.pk.clone(), state.addr);
                    if let Some(writer) = queue_conn.remove(&addr) {
                        tracing::info!(addr = %addr, "Can not send message");
                        let prot =
                            P2Protocol::new(&PeerMessage::HandshakeAck(handshake_ack.clone()));
                        if let Err(e) = Self::send(writer.clone(), &prot).await {
                            tracing::error!(addr = %addr, error = ?e, "Can not send message");
                        } else {
                            let peer = Peer::new(addr, hs.pk.clone());
                            if let Ok(peer_id) = peer.derive_unique_id() {
                                peers.lock().await.insert(peer_id, peer);
                                connections.insert(peer_id, writer);
                            } else {
                                tracing::error!(addr = %addr, "Can not derive PeerID");
                            }
                        }
                    }
                }
                ClientCommand::Pong(id) => {
                    if let Some(conn) = connections.get(&id) {
                        let prot = P2Protocol::new_with_peer(&PeerMessage::Pong, id);
                        if let Err(e) = Self::send(conn.clone(), &prot).await {
                            tracing::error!(id = %id, error = ?e, "Failed sending pong");
                        }
                    } else {
                        tracing::warn!(id = %id, "Peer not found!");
                    }
                }
                ClientCommand::Ping(id) => {
                    if let Some(conn) = connections.get(&id) {
                        let prot = P2Protocol::new_with_peer(&PeerMessage::Ping, id);
                        if let Err(e) = Self::send(conn.clone(), &prot).await {
                            tracing::error!(id = %id, error = ?e, "Failed sending ping");
                        }
                    }
                }
                ClientCommand::ConnectToPeer(peer) => {
                    match Self::connect_to_server(state.clone(), tx.clone(), peer.addr).await {
                        Ok(w_buf) => {
                            queue_conn.insert(peer.addr, Arc::new(Mutex::new(w_buf)));
                        }
                        Err(e) => {
                            tracing::error!(addr = %peer.addr, error = ?e, "Failed to connect");
                        }
                    }
                }
                ClientCommand::Broadcast(_message) => {}
            }
        }
        tracing::warn!("client task ended: channel closed");
        Ok(())
    }

    async fn connect_to_server(
        state: Arc<AppState>,
        client_tx: Arc<Sender<ClientCommand>>,
        addr: SocketAddr,
    ) -> Result<BufWriter<OwnedWriteHalf>> {
        let stream = TcpStream::connect(addr).await?;
        stream.set_nodelay(true)?;
        let (r, w) = stream.into_split();
        let reader = BufReader::new(r);
        let writer = BufWriter::new(w);

        tokio::spawn(async move {
            match Self::handle_incomming(state, reader, client_tx, addr).await {
                Ok(_) => {
                    tracing::warn!(addr = %addr, "Connection closed");
                }
                Err(e) => {
                    tracing::error!(error = ?e, "Error in handeling the connection");
                }
            }
        });
        Ok(writer)
    }

    pub async fn broadcast(&self, message: PeerMessage) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::Broadcast(message)).await?;
        Ok(())
    }

    pub async fn ping(&self, peer_id: u128) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::Ping(peer_id)).await?;
        Ok(())
    }

    pub async fn connect_to_peer(&self, peer: Peer) -> Result<()> {
        let Some(tx) = &self.client_tx else {
            anyhow::bail!("Broadcast channel is not ready yet!");
        };
        tx.send(ClientCommand::ConnectToPeer(peer)).await?;
        Ok(())
    }
}
