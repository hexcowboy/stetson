<h1 align="center">
  Stetson
</h1>

<p align="center">
  Stetson is a pure rust pubsub server over HTTP websockets.
</p>

<p align="center">
  Use it vanilla or fork it and make your own modifications.
</p>

<div align="center">
  <a href="https://crates.io/crates/stetson">
    <image src="https://img.shields.io/crates/v/stetson.svg" alt="Crates.io" />
  </a>
</div>


## Usage

Install the binary

```bash
cargo install stetson
```

Run the server, making sure to set a publisher key

```bash
echo "PUBLISH_KEY=$(openssl rand -hex 24)" > .env ; stetson
```

You can find the publisher key that was generated in the previous step in the `.env` file

```bash
cat .env
```

### Requests

`subscribe`

```json
{ "subscribe": { "topics": ["sports", "weather"] } }
```

`unsubscribe`

```json
{ "unsubscribe": { "topics": ["sports"] } }
```

`publish`

```json
{ "publish": { "topics": ["weather"], "message": "storms ahead", "key": "..." } }
```

### Responses

`message` - received when a new message from a subscribed topic is received

```json
{ "message": { "topic": "weather", "message": "storms ahead" } }
```

`error` - received when there was en error publishing a message

```json
{ "error": { "message": "some error message here" } }
```

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
