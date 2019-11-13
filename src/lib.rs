use bytes::{BytesMut, BufMut};
use futures::{Future, FutureExt, Stream, StreamExt};
use hyper::{client, http, Body, Client, Request, Response};
use hyper_tls::HttpsConnector;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_timer::{timer, Timer};
use tracing::{event, Level};

// TODO add timeout
pub struct Opts {
    reconnection_delay: u64, // in milliseconds
    buffer_capacity: usize,
}

impl EventSource {
    pub async fn new(url: &str) -> Result<EventSource, Box<dyn std::error::Error>> {
        // defaults
        let opts = Opts {
            reconnection_delay: 1000,
            buffer_capacity: 1_000_000,
        };

        Self::new_with_opts(url, &opts).await
    }

    pub async fn new_with_opts(url: &str, opts: &Opts) -> Result<EventSource, Box<dyn std::error::Error>> {
        let https = HttpsConnector::new()?;
        let client = Client::builder()
            .build::<_, hyper::Body>(https);

        let url: http::Uri = url.parse()?;

        let req = Request::builder()
            .method("GET")
            .uri(url.clone())
            .header("Accept", "text/event-stream")
            .body(Body::empty())?;


        let resp = client.request(req).await?;

        // TODO check that resp is `text/event-stream` content type
        // and that it's 200OK

        let body = resp.into_body();

        // This uses the default timer associated with the tokio runtime.
        // We don't have to manually set up a timer.
        let timer_handle = timer::Handle::default();

        Ok(EventSource {
            client,
            url,
            reconnection_delay: opts.reconnection_delay,
            last_event_id_string: "".to_owned(),
            timer: timer_handle,
            buf: BytesMut::with_capacity(opts.buffer_capacity),
            body,
            state: State::Stream,
        })
    }
}

pub struct EventSource {
    client: Client<HttpsConnector<client::HttpConnector>>,
    url: http::Uri,
    reconnection_delay: u64, // in milliseconds
    last_event_id_string: String,
    timer: timer::Handle,
    buf: BytesMut,
    body: Body,
    state: State,
}

impl Stream for EventSource {
    type Item = Result<Event, Box<dyn std::error::Error>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let EventSource {
            ref mut client,
            url,
            reconnection_delay,
            timer,
            ref mut buf,
            ref mut body,
            ref mut state,
            .. } = self.get_mut();

        // first handle reconnect and delay
        if let State::Delay(ref mut delay) = state {
            match delay.poll_unpin(cx) {
                Poll::Pending => {
                    event!(Level::DEBUG, "pending delay");
                    return Poll::Pending;
                },
                Poll::Ready(_) => {
                    event!(Level::DEBUG, "readyok, done delaying");
                    let req = Request::builder()
                        .method("GET")
                        .uri((*url).clone())
                        .header("Accept", "text/event-stream")
                        .body(Body::empty())?;

                    let resp_fut = client.request(req);

                    *state = State::Reconnect(Box::pin(resp_fut));
                },
            }
        }
        if let State::Reconnect(ref mut resp_fut) = state {
            match resp_fut.poll_unpin(cx) {
                Poll::Pending => {
                    event!(Level::DEBUG, "pending reconnect");
                    return Poll::Pending;
                },
                Poll::Ready(Err(err)) => {
                    event!(Level::DEBUG, "readyerr reconnect err: {}", err);
                    return Poll::Ready(Some(Err(format!("Could not reconnect: {}", err).into())));
                },
                Poll::Ready(Ok(resp)) => {
                    event!(Level::DEBUG, "readyok, reconnected");

                    // TODO 204 No Content means don't try to reconnect
                    *body = resp.into_body();
                    *state = State::Stream;
                },
            }
        }

        // Now handle stream
        match body.poll_next_unpin(cx) {
            Poll::Pending => { event!(Level::DEBUG, "pending"); Poll::Pending },
            Poll::Ready(None) => {
                event!(Level::DEBUG, "stream ended, reconnect with delay");
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(*reconnection_delay);
                let mut delay_fut = timer.delay(deadline);

                // We're still on the completed body (returned None), so have to kickstart
                // polling a new task? Doesn't seem to work if we just pass return Poll::Pending
                // and wait for the next round of polling.
                //
                // match block copied from above
                match delay_fut.poll_unpin(cx) {
                    Poll::Pending => {
                        event!(Level::DEBUG, "pending delay");
                        *state = State::Delay(Box::pin(delay_fut));
                        return Poll::Pending;
                    },
                    Poll::Ready(_) => {
                        event!(Level::DEBUG, "readyok, delaying");

                        // have to poll any other state that's not unit, there's no easy way to
                        // change state otherise
                        let req = Request::builder()
                            .method("GET")
                            .uri((*url).clone())
                            .header("Accept", "text/event-stream")
                            .body(Body::empty())?;

                        let mut resp_fut = client.request(req);

                        match resp_fut.poll_unpin(cx) {
                            Poll::Pending => {
                                event!(Level::DEBUG, "pending reconnect");
                                *state = State::Reconnect(Box::pin(resp_fut));
                                return Poll::Pending;
                            },
                            Poll::Ready(Err(err)) => {
                                event!(Level::DEBUG, "readyerr reconnect err: {}", err);
                                *state = State::Reconnect(Box::pin(resp_fut));
                                return Poll::Ready(Some(Err(format!("Could not reconnect: {}", err).into())));
                            },
                            Poll::Ready(Ok(resp)) => {
                                event!(Level::DEBUG, "readyok, reconnected");

                                // TODO 204 No Content means don't try to reconnect
                                *body = resp.into_body();
                                *state = State::Stream;
                                return Poll::Pending;
                            },
                        }
                    },
                }
            },
            Poll::Ready(Some(chunk)) => {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        // TODO check all error cases
                        if err.is_closed() || err.is_canceled() {
                            event!(Level::DEBUG, "error, reconnect with delay");

                            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(*reconnection_delay);
                            let delay_fut = timer.delay(deadline);

                            *state = State::Delay(Box::pin(delay_fut));

                            return Poll::Pending;
                        } else {
                            return Poll::Ready(Some(Err(err.into())));
                        }
                    },
                };

                buf.put(chunk.into_bytes());

                if let Some(idx) = buf.iter().rev().position(|byte| *byte == '\r' as u8 || *byte == '\n' as u8) {
                    let buf_len = buf.len();
                    let message_bytes = buf.split_to(buf_len - idx + 1);
                    event!(Level::DEBUG, "readysome, full message. idx: {}", idx);
                    Poll::Ready(Some(Ok(Event::Message(Message{
                        ty: "message".into(),
                        text: String::from_utf8(message_bytes.to_vec()).unwrap(),
                    }))))
                } else {
                    event!(Level::DEBUG, "readysome, not full message: {}", String::from_utf8(buf.to_vec()).unwrap());
                    Poll::Ready(None)
                }
            }
        }
    }
}

enum State {
    Stream,
    Reconnect(Pin<Box<dyn Future<Output=Result<Response<Body>, hyper::error::Error>>>>),
    Delay(Pin<Box<dyn Future<Output=()>>>),
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
