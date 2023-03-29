# Stetson

Stetson is a pure rust pubsub server. It uses HTTP and websockets to bring high speed subscriptions. Use it vanilla or fork it and make your own modifications.

## Usage

Install the binary and run the server:

```bash
cargo install stetson && stetson
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
{ "publish": { "topics": [""], "message": "hey", "key": "..." } }
```
