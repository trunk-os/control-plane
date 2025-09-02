use crate::{
    config::{Config, SocketConfig},
    server::{messages::*, Server},
};
use anyhow::{anyhow, Result};
use buckle::{config::ZFSConfig, testutil::make_server};
use rand::Fill;
use reqwest::Client;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use serde::{
    de::{Deserialize, DeserializeOwned},
    Serialize,
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tempfile::NamedTempFile;

pub async fn find_listener() -> Result<SocketAddr> {
    loop {
        let port: u16 = rand::random();
        let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
        match tokio::net::TcpListener::bind(addr).await {
            Ok(_) => return Ok(addr),
            _ => {}
        }
    }
}

pub async fn make_config(addr: Option<SocketAddr>, poolname: Option<String>) -> Result<Config> {
    std::fs::create_dir_all("tmp")?;
    let tf = NamedTempFile::new_in("tmp")?;
    let (_, dbfile) = tf.keep()?;

    let mut key: [u8; 64] = [0u8; 64];
    let mut salt: [u8; 32] = [0u8; 32];
    key.fill(&mut rand::rng());
    salt.fill(&mut rand::rng());

    Ok(Config {
        listen: if let Some(addr) = addr {
            addr
        } else {
            find_listener().await?
        },
        sockets: SocketConfig {
            buckle: make_server(if let Some(poolname) = poolname {
                Some(buckle::config::Config {
                    socket: buckle::testutil::find_listener()?,
                    zfs: ZFSConfig { pool: poolname },
                    log_level: buckle::config::LogLevel::Error,
                })
            } else {
                None
            })
            .await?,
            charon: start_charon("testdata/charon".into()).await?,
        },

        db: dbfile,
        signing_key: key.to_vec(),
        signing_key_salt: salt.to_vec(),
        log_level: buckle::config::LogLevel::Error,
    })
}

pub async fn start_server(poolname: Option<String>) -> Result<SocketAddr> {
    let addr = find_listener().await?;
    let ret = addr.clone();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let config = make_config(Some(addr), poolname).await.unwrap();
    let call = async move {
        Server::new(config).await.unwrap().start().await.unwrap();
    };
    tokio::spawn(call);
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    Ok(ret)
}

pub async fn start_charon(registry: PathBuf) -> Result<PathBuf> {
    std::fs::create_dir_all("tmp")?;
    let tf = NamedTempFile::new_in("tmp")?;
    let (_, path) = tf.keep()?;
    let p2 = path.clone();
    tokio::spawn(async move {
        charon::Server::new(charon::Config {
            registry: charon::RegistryConfig {
                path: registry,
                url: None,
            },
            socket: p2,
            log_level: None,
            debug: Some(true),
            systemd_root: None,
            charon_path: None,
            buckle_socket: "/tmp/buckled.sock".into(),
        })
        .start()
        .unwrap()
        .await
    });

    Ok(path)
}

pub struct TestClient {
    client: Client,
    baseurl: String,
    token: Option<String>,
}

impl TestClient {
    pub fn new(addr: SocketAddr) -> Self {
        let store = Arc::new(CookieStoreMutex::new(CookieStore::default()));
        Self {
            client: Client::builder().cookie_provider(store).build().unwrap(),
            baseurl: format!("http://{}", addr),
            token: None,
        }
    }

    pub async fn login(&mut self, input: Authentication) -> Result<()> {
        self.token = Default::default();
        let response = self
            .post::<Authentication, Token>("/session/login", input)
            .await?;
        self.token = Some(response.token);
        Ok(())
    }

    pub async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de> + DeserializeOwned + Default,
    {
        let mut req = self.client.get(&format!("{}{}", self.baseurl, path));

        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {}", token))
        }

        let resp = req.send().await?;

        if resp.status() != 200 {
            return Err(anyhow!(
                "{}",
                String::from_utf8(resp.bytes().await?.to_vec())?
            ));
        }

        let byt = resp.bytes().await?;

        if byt.len() > 0 {
            Ok(ciborium::from_reader(std::io::Cursor::new(byt))?)
        } else {
            Ok(T::default())
        }
    }

    pub async fn delete<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de> + DeserializeOwned + Default,
    {
        let mut req = self.client.delete(&format!("{}{}", self.baseurl, path));

        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {}", token))
        }

        let resp = req.send().await?;

        if resp.status() != 200 {
            return Err(anyhow!(
                "{}",
                String::from_utf8(resp.bytes().await?.to_vec())?
            ));
        }

        let byt = resp.bytes().await?;

        if byt.len() > 0 {
            Ok(ciborium::from_reader(std::io::Cursor::new(byt))?)
        } else {
            Ok(T::default())
        }
    }

    pub async fn post<I, O>(&self, path: &str, input: I) -> Result<O>
    where
        I: Serialize,
        O: for<'de> Deserialize<'de> + DeserializeOwned + Default,
    {
        let mut inner = Vec::with_capacity(65535);
        let mut buf = std::io::Cursor::new(&mut inner);
        ciborium::into_writer(&input, &mut buf)?;

        let mut req = self
            .client
            .post(&format!("{}{}", self.baseurl, path))
            .header("Content-type", "application/cbor");

        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {}", token))
        }

        let resp = req.body(buf.into_inner().to_vec()).send().await?;

        if resp.status() != 200 {
            return Err(anyhow!(
                "{}",
                String::from_utf8(resp.bytes().await?.to_vec())?
            ));
        }

        let byt = resp.bytes().await?;

        if byt.len() > 0 {
            Ok(ciborium::from_reader(std::io::Cursor::new(byt))?)
        } else {
            Ok(O::default())
        }
    }

    pub async fn put<I, O>(&self, path: &str, input: I) -> Result<O>
    where
        I: Serialize,
        O: for<'de> Deserialize<'de> + DeserializeOwned + Default,
    {
        let mut inner = Vec::with_capacity(65535);
        let mut buf = std::io::Cursor::new(&mut inner);
        ciborium::into_writer(&input, &mut buf)?;

        let mut req = self
            .client
            .put(&format!("{}{}", self.baseurl, path))
            .header("Content-type", "application/cbor");

        if let Some(token) = &self.token {
            req = req.header("Authorization", &format!("Bearer {}", token))
        }

        let resp = req.body(buf.into_inner().to_vec()).send().await?;

        if resp.status() != 200 {
            return Err(anyhow!(
                "{}",
                String::from_utf8(resp.bytes().await?.to_vec())?
            ));
        }

        let byt = resp.bytes().await?;

        if byt.len() > 0 {
            Ok(ciborium::from_reader(std::io::Cursor::new(byt))?)
        } else {
            Ok(O::default())
        }
    }
}
