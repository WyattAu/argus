//! Manual probe: connect to the Binance combined stream and print the first
//! three messages. Run: `cargo run -p mdkit --example wsprobe`

use futures_util::StreamExt;

#[tokio::main]
async fn main() {
    let url = "wss://stream.binance.com:9443/stream?streams=btcusdt@trade";
    println!("connecting to {url}");
    let (ws, _resp) = match tokio_tungstenite::connect_async(url).await {
        Ok(ws) => ws,
        Err(error) => {
            println!("connect failed: {error}");
            return;
        }
    };
    println!("connected");
    let (_write, mut read) = ws.split();
    let mut count = 0;
    while let Some(message) = read.next().await {
        match message {
            Ok(msg) => {
                println!("msg: {msg}");
                count += 1;
            }
            Err(error) => {
                println!("read error: {error}");
                return;
            }
        }
        if count >= 3 {
            return;
        }
    }
}
