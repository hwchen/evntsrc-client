use evntsrc_client::{Event, EventSource};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::args().nth(1).expect("please enter url");

    // logging
    tracing_subscriber::fmt::init();

    let mut es = EventSource::new(&url).await?;

    while let Some(event) = es.next().await {
        let event = event?;

        match event {
            Event::Message(msg)=> {
                println!("type: {}", msg.ty);
                println!("text: {}", msg.text);
            },
            _ => println!("only messages supported"),
        }
    };

    Ok(())
}
