# Reference tasks in the XCTSK format

`task.xctsk` is a made-up task written by hand to the format XCTrack
publishes at <https://xctrack.org/Competition_Interfaces.html>.

The other three files are what XCTrack's own task tool at
<https://tools.xcontest.org/xctsk> made of it on 2026-11-19:

- `task-v2.json`: the compact form, from `/api/xctsk/loadV2/`.
- `task-qr.txt`: the text of the QR code from `/api/xctsk/qr`.
- `task-qr-compressed.txt`: the text of the QR code from `/api/xctsk/qr2z`.

They are the evidence for how the compact form writes a turnpoint, a goal
line and a start, that it drops the take-off mark of the first turnpoint,
and that the compressed form is zlib in standard base64.
