mod events;
mod parse;

use bytes::{BytesMut, BufMut};
use futures::{Future, FutureExt, Stream, StreamExt};
use hyper::{client, http, Body, Client, Request, Response};
use hyper_tls::HttpsConnector;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_timer::timer;
use tracing::{event, Level};

use crate::parse::{Line, Field};
pub use crate::events::Event;

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
        //
        // https://github.com/tokio-rs/tokio/issues/1466#issuecomment-522300040
        // and then read `Handle` documentation to understand that a default handle
        // will reference whatever timer is available.
        // I spent a lot of hours figuring this out :sweat:, along with the polling.
        // The docs are not super clear, and there's not really many examples.
        let timer_handle = timer::Handle::default();

        Ok(EventSource {
            client,
            url,
            reconnection_delay: opts.reconnection_delay,
            last_id: "".to_owned(),
            timer: timer_handle,
            buf: BytesMut::with_capacity(opts.buffer_capacity),
            body,
            state: State::Stream,
            event_buf: Event::default(),
        })
    }
}

pub struct EventSource {
    client: Client<HttpsConnector<client::HttpConnector>>,
    url: http::Uri,
    reconnection_delay: u64, // in milliseconds
    last_id: String,
    timer: timer::Handle,
    buf: BytesMut,
    body: Body,
    state: State,
    event_buf: Event,
}

impl Stream for EventSource {
    type Item = Result<Event, Box<dyn std::error::Error>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let EventSource {
            ref mut client,
            url,
            reconnection_delay,
            ref mut last_id,
            timer,
            ref mut buf,
            ref mut body,
            ref mut state,
            ref mut event_buf,
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
                // note: whitespace trimmed when parsing line
                if let Some(idx) = buf.iter().rev().position(|byte| *byte == '\r' as u8 || *byte == '\n' as u8) {
                    let message_bytes = buf.split_to(buf.len() - idx + 1);
                    event!(Level::DEBUG, "readysome, full lines. chunk idx: {}", idx);

                    let s = String::from_utf8(message_bytes.to_vec())?;
                    event!(Level::INFO, "process lines: {}", s);

                    // fn process line
                    match s.parse::<Line>() {
                        Ok(line) => {
                            match line {
                                Line::Empty => {
                                    event!(Level::DEBUG, "dispatch event: {}", s);
                                    let dispatched_event = std::mem::take(event_buf);
                                    return Poll::Ready(Some(Ok(dispatched_event)));
                                },
                                Line::Comment(s) => {
                                    event!(Level::INFO, "comment: {}", s);
                                    return Poll::Pending;
                                },
                                Line::FieldValue { field, value } => {
                                    // fn process field
                                    match field {
                                        Field::Event => {
                                            event!(Level::DEBUG, "update event type: {}", s);
                                            event_buf.ty = value;
                                            return Poll::Pending;
                                        },
                                        Field::Data => {
                                            event!(Level::DEBUG, "update event data: {}", s);
                                            event_buf.data = value;
                                            return Poll::Pending;
                                        },
                                        Field::Id => {
                                            event!(Level::DEBUG, "update event id: {}", s);
                                            *last_id = value.clone();
                                            event_buf.id = value;
                                            return Poll::Pending;
                                        },
                                        Field::Retry => {
                                            let new_delay = match value.parse::<u64>() {
                                                Ok(t) => t,
                                                Err(_) => return Poll::Pending,//ignore
                                            };
                                            *reconnection_delay = new_delay;
                                            return Poll::Pending;
                                        },
                                    }
                                },
                            }
                        },

                        Err(err) => Poll::Ready(Some(Err(err.into()))),
                    }
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

