"""Sends a receipt for every order the API places, through the relay in config.yaml."""

import json
import smtplib
import time
from dataclasses import dataclass
from email.message import EmailMessage

RELAY = ("localhost", 2525)
POLL_SECONDS = 5


@dataclass
class Receipt:
    order_id: str
    email: str
    total_cents: int

    def subject(self) -> str:
        return f"Your Lantern order {self.order_id[:8]}"

    def body(self) -> str:
        return f"Thank you! You paid ${self.total_cents / 100:.2f}.\n"


def send(receipt: Receipt) -> None:
    message = EmailMessage()
    message["From"] = "shop@lantern.example"
    message["To"] = receipt.email
    message["Subject"] = receipt.subject()
    message.set_content(receipt.body())
    with smtplib.SMTP(*RELAY) as smtp:
        smtp.send_message(message)


def main() -> None:
    while True:
        for line in open("outbox.jsonl", encoding="utf-8"):
            send(Receipt(**json.loads(line)))
        time.sleep(POLL_SECONDS)


if __name__ == "__main__":
    main()
