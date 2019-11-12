Client library for EventSource (aka Server-Sent Events).

[Specification](https://html.spec.whatwg.org/multipage/server-sent-events.html#server-sent-events)

This crate is intended to integrate with an async runtime. For now, I've chosen to start with `tokio`. In addition, it's based on `std::futures` and allows use of rust's `async/await` syntax.

The API is a bit more 'rust' than 'js'. It exposes an `EventStream` which you can iterate over, and `match` on individual events to dispatch handlers. This is a little different from the javascript API which attaches listeners (where you can register handlers) to the event stream.
