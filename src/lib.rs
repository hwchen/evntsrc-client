use bytes::{BytesMut, BufMut};
use futures::{FutureExt, Stream, StreamExt};
use hyper::{client, http, Body, Client, Request};
use hyper_tls::HttpsConnector;
use std::pin::Pin;
use std::task::{Context, Poll};

// TODO change to Opts
pub struct EventSource {
    url: String,
    reconnection_time: u32, // in milliseconds
    last_event_id_string: String,
}

impl EventSource {
    pub fn new(url: String) -> Self {
        EventSource {
            url,
            reconnection_time: 0,
            last_event_id_string: "".to_owned(),
        }
    }

    pub async fn stream(self) -> Result<EventStream, Box<dyn std::error::Error>> {
        let https = HttpsConnector::new()?;
        let client = Client::builder()
            .build::<_, hyper::Body>(https);

        let url: http::Uri = self.url.parse()?;

        let req = Request::builder()
            .method("GET")
            .uri(url.clone())
            .header("Accept", "text/event-stream")
            .body(Body::empty())?;


        let resp = client.request(req).await?;

        // TODO check that resp is `text/event-stream` content type
        // and that it's 200OK
        // 204 No Content means don't try to reconnect

        let body = resp.into_body();

        Ok(EventStream {
            client,
            url,
            reconnection_time: 0,
            last_event_id_string: "".to_owned(),
            buf: BytesMut::with_capacity(1_000_000),
            body,
        })
    }
}

pub struct EventStream {
    client: Client<HttpsConnector<client::HttpConnector>>,
    url: http::Uri,
    reconnection_time: u32, // in milliseconds
    last_event_id_string: String,
    buf: BytesMut,
    body: Body,
}

impl Stream for EventStream {
    type Item = Result<Event, Box<dyn std::error::Error>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let EventStream{ ref mut body, ref mut buf, ref mut client, url, .. }  = self.get_mut();

        match body.poll_next_unpin(cx) {
            Poll::Pending => { println!("pending"); Poll::Pending},
            Poll::Ready(None) => {
                println!("readynone, reconnect");

                let req = Request::builder()
                    .method("GET")
                    .uri((*url).clone())
                    .header("Accept", "text/event-stream")
                    .body(Body::empty())?;


                // how to deal with await here? manually poll?
                let mut resp_fut = client.request(req);

                // TODO will this loop block?
                loop {
                    match resp_fut.poll_unpin(cx) {
                        Poll::Pending => {
                            //println!("pending reconnect");
                            continue;
                        },
                        Poll::Ready(Err(err)) => {
                            println!("readyerr reconnect err: {}", err);
                            return Poll::Ready(Some(Err(format!("Could not reconnect: {}", err).into())));
                        },
                        Poll::Ready(Ok(resp)) => {
                            println!("readyok, reconnecting");
                            *body = resp.into_body();
                            break;
                        },
                    }
                }

                Poll::Pending
            },
            Poll::Ready(Some(chunk)) => {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(err) => return Poll::Ready(Some(Err(err.into()))),
                };

                buf.put(chunk.into_bytes());

                if let Some(idx) = buf.iter().rev().position(|byte| *byte == '\r' as u8 || *byte == '\n' as u8) {
                    let buf_len = buf.len();
                    let message_bytes = buf.split_to(buf_len - idx + 1);
                    println!("readysome, full message. idx: {}", idx);
                    Poll::Ready(Some(Ok(Event::Message(Message{
                        ty: "message".into(),
                        text: String::from_utf8(message_bytes.to_vec()).unwrap(),
                    }))))
                } else {
                    println!("readysome, not full message");
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
