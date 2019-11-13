use evntsrc_client::EventSource;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::args().nth(1).expect("please enter url");

    // logging
    tracing_subscriber::fmt::init();

    let mut ev = EventSource::new(&url).await?;

    while let Some(event) = ev.next().await {
        let event = event?;
        println!("event: {:#?}", event);
    };

    Ok(())
}
