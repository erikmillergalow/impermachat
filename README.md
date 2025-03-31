# Impermachat

Ephemeral chat rooms with real time typing display between all participants.

[Try it out here](https://impermachat.emgemg.net/)

![Chat rooms example](hosting/impermachat-example.gif "impermachat-example")

Built with [Axum](https://github.com/tokio-rs/axum), [Askama](https://github.com/askama-rs/askama), [Datastar](https://data-star.dev/), and [missing.css](https://missing.style/)

### Hot reload for dev
`systemfd --no-pid -s http::8080 -- cargo watch -x run`

[View on Forgejo](https://impermachat.emgemg.net/)
