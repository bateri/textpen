# Lantern

A small shop API and the worker that sends its mail. It is here to be edited: open it with
`textpen` and look around.

## Run it

```sh
npm install
npm run dev          # the API on :8080
python3 worker/mailer.py
```

## What is where

| Path              | What                                   |
| ----------------- | -------------------------------------- |
| `src/server.ts`   | the HTTP server and its routes         |
| `src/lib/`        | money, slugs and the order book        |
| `worker/`         | the mailer, in Python                  |
| `config.yaml`     | ports, the database, the mail relay    |

> Prices are kept in cents, never in floats.

- [x] orders
- [x] receipts by mail
- [ ] refunds
