# Impermachat

Ephemeral chat rooms with real time typing display between all participants.

![Chat rooms example](https://git.emgemg.net/emgemg/axum-datastar-realtime/src/commit/4895239e2de1c4da1ed15b4f6e0de48b1317b066/hosting/impermachat-example.gif "impermachat-example")

Built with [Axum](https://github.com/tokio-rs/axum), [Askama](https://github.com/askama-rs/askama), [Datastar](https://data-star.dev/), and [missing.css](https://missing.style/)

### Hot reload for dev
`systemfd --no-pid -s http::8080 -- cargo watch -x run`
