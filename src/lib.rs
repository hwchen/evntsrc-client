use bytes::{BytesMut, BufMut};
use futures::{Stream, StreamExt};
use hyper::{http, Body, Client};
use hyper_tls::HttpsConnector;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct EventSource {
    url: String,
    request: Option<()>,
    reconnection_time: u32, // in milliseconds
    last_event_id_string: String,
}

impl EventSource {
    pub fn new(url: String) -> Self {
        EventSource {
            url,
            request: None,
            reconnection_time: 0,
            last_event_id_string: "".to_owned(),
        }
    }

    pub async fn stream(self) -> Result<EventStream, Box<dyn std::error::Error>> {
        let https = HttpsConnector::new()?;
        let client = Client::builder()
            .build::<_, hyper::Body>(https);

        let resource: http::Uri = self.url.parse()?;

        let resp = client.get(resource).await?;

        // TODO check that resp is `text/event-stream` content type
        // and that it's 200OK

        let body = resp.into_body();

        Ok(EventStream {
            buf: BytesMut::with_capacity(1_000_000),
            body,
            request: None,
            reconnection_time: 0,
            last_event_id_string: "".to_owned(),
        })
    }
}

pub struct EventStream {
    buf: BytesMut,
    body: Body,
    request: Option<()>,
    reconnection_time: u32, // in milliseconds
    last_event_id_string: String,
}

impl Stream for EventStream {
    type Item = Result<Event, Box<dyn std::error::Error>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let EventStream{ ref mut body, ref mut buf, .. }  = self.get_mut();

        match body.poll_next_unpin(cx) {
            Poll::Pending => { eprintln!("pending"); Poll::Pending},
            Poll::Ready(None) => { eprintln!("readynone"); Poll::Ready(None)},
            Poll::Ready(Some(chunk)) => {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(err) => return Poll::Ready(Some(Err(err.into()))),
                };

                buf.put(chunk.into_bytes());
                eprintln!("readysome: {}", String::from_utf8(buf.to_vec()).unwrap());

                if let Some(idx) = buf.iter().rev().position(|byte| *byte == '\r' as u8 || *byte == '\n' as u8) {
                    let buf_len = buf.len();
                    let message_bytes = buf.split_to(buf_len - idx + 1);
                    eprintln!("readysome, full message. idx: {}", idx);
                    Poll::Ready(Some(Ok(Event::Message(Message{
                        ty: "message".into(),
                        text: String::from_utf8(message_bytes.to_vec()).unwrap(),
                    }))))
                } else {
                    eprintln!("readysome, not full message");
                    Poll::Ready(None)
                }
            }
        }
    }
}

pub enum Event {
    Message(Message),
    Comment,
}

pub struct Message {
    pub ty: String,
    pub text: String,
}


// parse

enum Field {
    Event,
    Data,
    Id,
    Retry,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
