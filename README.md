Client library for EventSource (aka Server-Sent Events).

[Specification](https://html.spec.whatwg.org/multipage/server-sent-events.html#server-sent-events)

This crate is intended to integrate with an async runtime. For now, I've chosen to start with `tokio`. In addition, it's based on `std::futures` and allows use of rust's `async/await` syntax.

The API is a bit more 'rust' than 'js'. It exposes an `EventStream` which you can iterate over, and `match` on individual events to dispatch handlers. This is a little different from the javascript API which attaches listeners (where you can register handlers) to the event stream.

This is also an experiment in implementing streams and futures.

Some of the stuff I learned:
- how to poll future/stream inside of an implementation of `Future`/`Stream` (don't use a hot loop, I ended up using a state machine, I think this answer came somewhere on stackoverflow, can't find it now).
- working with `Pin`, although I still don't understand it deeply. This took a long time because the error message was just that the method 'poll' was not available, without any suggestions. I ended up using `poll_unpin`.
- using `tokio::timer` inside of a `Future`/`Stream` implementation. This one took several hours, I went down a lot of dead ends. The docs were definitely not enough for me, and the api has changed quite a bit. A working example would probably have been really helpful. This [issue](https://github.com/tokio-rs/tokio/issues/1466#issuecomment-522300040) provided the aha moment. Fortunately, since I was using the tokio runtime, the solution was simple.
- wrapping a stream within a stream, in order to flatten out chunked input. It was fun to poke at the docs for `stream::flatten`. Not too hard, just took a while to decide that this was the (seemingly) best way.

Code is not currently structure very well, feedback is welcome.
