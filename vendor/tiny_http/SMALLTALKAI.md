This directory contains tiny_http 0.12.0 from crates.io, under its original
MIT or Apache-2.0 license. The application uses a local patch to bound HTTP
ingress before its route handlers run. The transport serves one request per
connection and keeps streaming responses open until their writer finishes.
The patch caps active connections, header size and count, and request body
size and duration. Rejecting an unread body closes its connection without
draining it. The reader and Settings servers use the same patched dependency.
