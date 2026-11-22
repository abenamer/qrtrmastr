#!/usr/bin/env python3
"""Write the seed corpus of the fuzz targets from the synthetic fixtures (REAS-TEST-003).

    python3 scripts/fuzz_seeds.py            write fuzz/seeds/<target>/<name>
    python3 scripts/fuzz_seeds.py --check    fail when the committed seeds are not what this
                                             script writes: a seed that was edited by hand, a
                                             seed that is missing, a file that is not a seed

The seeds are derived files (docs/reference/files.md): change this script or a synthetic
fixture and run the script; do not edit a seed. Each target has at least one seed that its
reader accepts. Every value in a seed is made up: the person of the synthetic receipts, with a
reserved mail domain, a phone number of the range for fiction and an invented street.

The bytes are the same on every machine: the archive is put together here byte by byte, with
its entries in deflate blocks of the stored kind, so no compression library decides them.
"""
from __future__ import annotations

import base64
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEEDS = ROOT / "fuzz" / "seeds"
SYNTHETIC = ROOT / "tests" / "fixtures" / "synthetic" / "input" / "freshdirect"
TARGETS = ("takeout", "mime", "classify", "redact", "receipt", "pack", "gmail_json")

RECEIPT_SUBJECT = "Your FreshDirect order is in the works! Here's your receipt"


def mail(html: str, message_id: str = "r1@mail.example", sender: str = "FreshDirect <service@freshdirect.com>",
         date: str | None = "Tue, 3 Mar 2026 06:15:00 -0500") -> bytes:
    """One mail with an HTML body, as the import tests build it."""
    headers = [
        f"From: {sender}",
        "To: pat.example@example.com",
        f"Subject: {RECEIPT_SUBJECT}",
    ]
    if date is not None:
        headers.append(f"Date: {date}")
    headers += [
        f"Message-ID: <{message_id}>",
        "Authentication-Results: mx.google.com; dkim=pass header.i=@freshdirect.com",
        "Content-Type: text/html; charset=utf-8",
    ]
    return ("\r\n".join(headers) + "\r\n\r\n" + html).encode("utf-8")


def multipart() -> bytes:
    """A mail of two parts, text first, as a mail client writes it."""
    boundary = "seed-boundary"
    lines = [
        "From: FreshDirect <service@freshdirect.com>",
        "To: Pat Example <pat.example@example.com>",
        "Subject: Your order for Tuesday",
        "Date: Tue, 3 Mar 2026 06:15:00 -0500",
        "Message-ID: <multi@mail.example>",
        "MIME-Version: 1.0",
        f'Content-Type: multipart/alternative; boundary="{boundary}"',
        "",
        f"--{boundary}",
        "Content-Type: text/plain; charset=utf-8",
        "",
        "Hi Pat, your order 12345678901 arrives Tuesday, March 3, 2026.",
        f"--{boundary}",
        "Content-Type: text/html; charset=utf-8",
        "Content-Transfer-Encoding: quoted-printable",
        "",
        "<html><body><p>Hi Pat, your order <b>12345678901</b> arrives=20Tuesday.</p></body></html>",
        f"--{boundary}--",
        "",
    ]
    return "\r\n".join(lines).encode("utf-8")


def mailbox(messages: list[bytes]) -> bytes:
    """A mailbox file: each message after a line that starts with `From `."""
    out = b""
    for number, message in enumerate(messages):
        out += f"From sender{number}@mail.example Tue Mar  3 06:15:00 2026\r\n".encode()
        out += message + b"\r\n\r\n"
    return out


def stored_deflate(data: bytes) -> bytes:
    """`data` as a deflate stream of stored blocks: valid for any reader, and the same bytes
    with every compression library."""
    out = b""
    blocks = [data[i:i + 0xFFFF] for i in range(0, len(data), 0xFFFF)] or [b""]
    for index, block in enumerate(blocks):
        last = 1 if index == len(blocks) - 1 else 0
        out += struct.pack("<BHH", last, len(block), len(block) ^ 0xFFFF) + block
    return out


def archive(entries: list[tuple[str, bytes]]) -> bytes:
    """A zip archive of the entries, each with the deflate method, with fixed dates."""
    local = b""
    central = b""
    for name, data in entries:
        raw_name = name.encode("utf-8")
        packed = stored_deflate(data)
        crc = zlib.crc32(data) & 0xFFFFFFFF
        offset = len(local)
        # version 2.0, no flags, method 8 (deflate), time 0, date 1980-01-01
        common = struct.pack("<HHHHHIII", 20, 0, 8, 0, 0x21, crc, len(packed), len(data))
        local += b"PK\x03\x04" + common + struct.pack("<HH", len(raw_name), 0) + raw_name + packed
        central += (b"PK\x01\x02" + struct.pack("<H", 20) + common
                    + struct.pack("<HHHHHII", len(raw_name), 0, 0, 0, 0, 0, offset) + raw_name)
    end = b"PK\x05\x06" + struct.pack("<HHHHIIH", 0, 0, len(entries), len(entries), len(central), len(local), 0)
    return local + central + end


def seeds() -> dict[str, bytes]:
    """Every seed, by its path below fuzz/seeds/."""
    basic = (SYNTHETIC / "2026" / "receipt-basic.html").read_text(encoding="utf-8")
    two_stores = (SYNTHETIC / "2024" / "receipt-two-stores.html").read_text(encoding="utf-8")
    receipt_mail = mail(basic)
    small_mail = mail("<p>Order 12345678902</p>", message_id="r2@mail.example")
    one_mailbox = mailbox([small_mail, mail("<p>Order 12345678903</p>", message_id="r3@mail.example")])
    raw = base64.urlsafe_b64encode(small_mail).decode("ascii")
    out = {
        # The Takeout reader: a mailbox file, an archive with a mailbox, an archive without one.
        "takeout/mailbox-two-messages": one_mailbox,
        "takeout/archive-one-mailbox": archive([
            ("Takeout/Mail/readme.txt", b"an export of mail\n"),
            ("Takeout/Mail/FreshDirect.mbox", one_mailbox),
        ]),
        "takeout/archive-no-mailbox": archive([("Takeout/Mail/readme.txt", b"no mail here\n")]),
        # The reader of one mail.
        "mime/receipt": receipt_mail,
        "mime/two-parts": multipart(),
        "mime/other-sender": mail("<p>hello</p>", sender="Someone <someone@mail.example>"),
        "mime/no-date": mail("<p>Order 12345678904</p>", message_id="r4@mail.example", date=None),
        "mime/key-with-a-term": mail("<p>x</p>", message_id="pat.example@example.com"),
        # Three keys that the check of a short text did not refuse before 2026-11-21.
        "mime/key-with-a-phone-in-a-longer-number": mail("<p>x</p>", message_id="r21255501991@mail.example"),
        "mime/key-with-a-percent-sign-before-a-phone": mail("<p>x</p>", message_id="x%2125550199@mail.example"),
        "mime/key-with-a-term-in-brackets": mail("<p>x</p>", message_id="a<pat.example@example.com>b@mail.example"),
        # The classifier: a subject, an address, and headers of results, one in a line.
        "classify/freshdirect-receipt": (
            f"{RECEIPT_SUBJECT}\nservice@freshdirect.com\n"
            "mx.google.com; dkim=pass header.i=@freshdirect.com; spf=pass\n"
        ).encode(),
        "classify/doordash-updated": (
            "Your order was updated\nNo-Reply@DoorDash.com \n"
            "mx.google.com;\n dkim=pass header.d=doordash.com\nother.example; dkim=pass header.d=doordash.com\n"
        ).encode(),
        "classify/no-pass": b"Thanks for your order\norders@eat.seamless.com\nmx.google.com; dkim=fail header.i=@seamless.com\n",
        # The redactor: a receipt, and a text with each kind of term.
        "redact/receipt": basic.encode("utf-8"),
        "redact/text-with-terms": (
            "Hi Pat Example, we deliver to 123 Imaginary Street Apt 4B, Brooklyn, NY 11201.\n"
            "Call (212) 555-0199 or write to pat.example@example.com. Card ending in 4242.\n"
            "P a t  E x a m p l e, 212.555.0199, PAT.EXAMPLE%40EXAMPLE.COM\n"
        ).encode(),
        "redact/markup-with-terms": (
            '<p>Hi <b>Pat</b> <i>Example</i></p><a href="https://shop.example/u/pat.example@example.com?x=1">'
            'account</a><img src="https://shop.example/open.gif" alt="Pat Example"><!-- Pat Example -->'
            "<p>123&nbsp;Imaginary&#32;Street Apt 4B</p>"
        ).encode(),
        "redact/short-texts": (
            "r21255501991@mail.example\nx%2125550199@mail.example\nx%123 Imaginary Street Apt 4B\n"
            "a<pat.example@example.com>b@mail.example\n/u/Pat%20Example\n"
        ).encode(),
        "redact/nothing-to-redact": b"Thanks for your order. Your driver is on the way.\n",
        # The receipt parser: the two synthetic receipts.
        "receipt/basic-2026": basic.encode("utf-8"),
        "receipt/two-stores-2024": two_stores.encode("utf-8"),
        # The JSON of the Gmail adapter: a page of the list, the last page, a message.
        "gmail_json/list-page": b'{"messages":[{"id":"18c2a9f0e1","threadId":"18c2a9f0e1"},{"id":"18c2a9f0e2","threadId":"t"}],"nextPageToken":"0123","resultSizeEstimate":2}',
        "gmail_json/list-last-page": b'{"resultSizeEstimate":0}',
        "gmail_json/message": ('{"id":"18c2a9f0e1","threadId":"18c2a9f0e1","raw":"' + raw + '"}').encode("ascii"),
        "gmail_json/message-with-padding": b'{"raw":"aGk=","sizeEstimate":2}',
    }
    for number, size in enumerate(("36ct, 0.17oz ea", "1/2 gallon", "approx. 1.5lb", "10-12ct", "375ml", "1 dozen", "64\ufb02 oz", "0ct")):
        out[f"pack/size-{number}"] = size.encode("utf-8")
    return out


def main(argv: list[str]) -> int:
    wanted = seeds()
    missing_target = [target for target in TARGETS if not any(name.startswith(target + "/") for name in wanted)]
    if missing_target:
        print("FAIL: no seed for the target", ", ".join(missing_target))
        return 1
    if argv == ["--check"]:
        errors = []
        present = {path.relative_to(SEEDS).as_posix() for path in SEEDS.rglob("*") if path.is_file()} if SEEDS.is_dir() else set()
        for name in sorted(set(wanted) - present):
            errors.append(f"fuzz/seeds/{name}: the seed is missing")
        for name in sorted(present - set(wanted)):
            errors.append(f"fuzz/seeds/{name}: not a seed that the script writes")
        for name in sorted(set(wanted) & present):
            if (SEEDS / name).read_bytes() != wanted[name]:
                errors.append(f"fuzz/seeds/{name}: not the bytes that the script writes")
        for error in errors:
            print("ERROR:", error)
        if errors:
            print("FAIL: run python3 scripts/fuzz_seeds.py, and do not edit a seed by hand")
            return 1
        print(f"PASS: {len(wanted)} seeds for {len(TARGETS)} targets are what scripts/fuzz_seeds.py writes")
        return 0
    if argv:
        print("usage: scripts/fuzz_seeds.py [--check]")
        return 2
    for name, data in wanted.items():
        path = SEEDS / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"wrote {len(wanted)} seeds for {len(TARGETS)} targets below fuzz/seeds/")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
