---
type: Architecture Decision
title: The next order as a page that is a file
description: Why the web page of qm due is one self-contained HTML file that the tool prints and no server, what the page shows, and what it cannot do.
tags: [decision, recommendation, cli]
generated: { by: "process:claude-code", at: "2026-11-19T17:00:00Z" }
updated: 2026-11-20
---

# ADR 0012 — The next order as a page that is a file

## Context

The maintainer asked for a web page with the latest recommendation and the other possible choices. The tool has no server: it is a command that runs and ends, and its container has no published port.

## Decision

* `qm due --html` prints the lists of [ADR 0011](/decisions/0011-essentials-extras-reminders-and-additions-with-a-reason.md) as one HTML document (`render_habit_page` in `src/adapters/cli/page.rs`). `scripts/due_page.sh` writes it to `next-order.html` in the exports directory, outside the repository, with 40 extras.
* The page shows the essentials and the additions ticked, as the recommendation, and the extras and the reminders not ticked, as the other choices. The stocked products are listed without a box. A button copies the names of the ticked products.
* The page is self-contained: styles and script inline, a content security policy that admits the inline style and script and nothing else (`default-src 'none'`, no base address, no form target), no link and no request to anywhere. Every product name and item text is escaped.
* The policy stops a load from anywhere; it does not stop a script written into the page, because it admits inline script. The one control against that is the escaping of every product name and every addition where the page is written, and a test holds it.
* The page is as fresh as its last run. It says the day it was made for.

## Consequences

* The page cannot add an item or record a reason; `qm add` does that. The ticks are not stored: a reload shows the page as it was written.
* The file holds the purchase history of the user in clear text. It lives where the exports live.
* No endpoint exists, so no endpoint needs authentication (REAS-SEC-001).

## Alternatives

* A server in the tool (`qm serve`) with a form for additions and their reasons: the step after this one, when the page proves useful. It adds a dependency, a listening port, and the questions of REAS-SEC-001 for each endpoint.
* A page hosted outside the machine: rejected; the purchase history would leave it.

## Runs

2026-11-19: the unit test `the_page_holds_every_list_escapes_its_texts_and_loads_nothing`; `scripts/due_page.sh` on the stored history wrote a page of 21 kB with 55 rows in five sections. A screenshot of that page from headless Chromium at 900 pixels wide showed the lists, the ticked essentials and the bar of buttons; the buttons were not clicked.
