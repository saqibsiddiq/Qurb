//! `host <state-dir>`: start Tor, publish an onion service, echo every
//! stream. `client <onion> <state-dir>`: start Tor, reach it, time a round
//! trip and 8 MiB each way. See ./README.md.
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt().with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".into())).init();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("host") => host(&args[2]).await,
        Some("client") => client(&args[2], &args[3]).await,
        _ => anyhow::bail!("host <dir> | client <onion> <dir>"),
    }
}

fn config(dir: &str) -> anyhow::Result<arti_client::TorClientConfig> {
    let mut b = arti_client::config::TorClientConfigBuilder::from_directories(format!("{dir}/state"), format!("{dir}/cache"));
    b.address_filter().allow_onion_addrs(true);
    Ok(b.build()?)
}

async fn host(dir: &str) -> anyhow::Result<()> {
    let started = Instant::now();
    let tor = arti_client::TorClient::create_bootstrapped(config(dir)?).await?;
    println!("bootstrapped in {:?}", started.elapsed());
    let svc = tor_hsservice::config::OnionServiceConfigBuilder::default().nickname("qurbspike".parse()?).build()?;
    let (service, requests) = tor.launch_onion_service(svc)?.ok_or_else(|| anyhow::anyhow!("not launched"))?;
    println!("onion {}", service.onion_address().map(|a| { use safelog::DisplayRedacted; a.display_unredacted().to_string() }).unwrap_or_default());
    let mut streams = tor_hsservice::handle_rend_requests(requests);
    while let Some(request) = streams.next().await {
        tokio::spawn(async move {
            let Ok(mut s) = request.accept(tor_cell::relaycell::msg::Connected::new_empty()).await else { return };
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match s.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => { if s.write_all(&buf[..n]).await.is_err() { break; } let _ = s.flush().await; }
                }
            }
        });
    }
    Ok(())
}

async fn client(onion: &str, dir: &str) -> anyhow::Result<()> {
    let started = Instant::now();
    let tor = arti_client::TorClient::create_bootstrapped(config(dir)?).await?;
    println!("bootstrapped in {:?}", started.elapsed());
    let t = Instant::now();
    let mut s = tor.connect((onion, 80)).await?;
    println!("reached the onion service in {:?}", t.elapsed());
    for i in 0..3 {
        let t = Instant::now();
        s.write_all(b"ping").await?; s.flush().await?;
        let mut b = [0u8; 4]; s.read_exact(&mut b).await?;
        println!("round trip {i}: {:?}", t.elapsed());
    }
    let data = vec![7u8; 8 << 20];
    let t = Instant::now();
    let (mut r, mut w) = s.split();
    let writer = async { for c in data.chunks(64 * 1024) { w.write_all(c).await?; } w.flush().await?; anyhow::Ok(()) };
    let reader = async { let mut got = 0; let mut b = vec![0u8; 64 * 1024]; while got < data.len() { let n = r.read(&mut b).await?; if n == 0 { break } got += n; } anyhow::Ok(got) };
    let (_, got) = tokio::try_join!(writer, reader)?;
    let secs = t.elapsed().as_secs_f64();
    println!("echoed {} MiB in {:.1} s: {:.2} MB/s each way", got >> 20, secs, got as f64 / secs / 1e6);
    Ok(())
}
