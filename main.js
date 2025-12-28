#!/usr/bin/env -S deno run --allow-write

import { TextLineStream } from "jsr:@std/streams/text-line-stream";

const cmpBy = (fn) => (a, b, aa = fn(a), bb = fn(b)) =>
  aa < bb ? -1 : aa > bb ? 1 : 0;

const url = (raw, ...parts) =>
  String.raw({ raw }, ...parts.map(encodeURIComponent));

class HTMLString extends String {}

const escape = (string) =>
  string.replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;").replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;").replaceAll("'", "&apos;");

const html = (raw, ...parts) =>
  new HTMLString(
    String.raw(
      { raw },
      ...parts.map((v) =>
        [v].flat().map((v) => v instanceof HTMLString ? v : escape(v)).join("")
      ),
    ),
  );

const events = (await Array.fromAsync(
  Deno.stdin.readable
    .pipeThrough(new TextDecoderStream())
    .pipeThrough(new TextLineStream()),
  (v) => JSON.parse(v),
)).sort(cmpBy((e) => e.origin_server_ts));

for (const [room, eventsInRoom] of Map.groupBy(events, (e) => e.room_id)) {
  Deno.writeTextFileSync(
    `logs/${room}.html`,
    html`
      <!DOCTYPE html>
      <style>
      :has(#table:checked) main {
        display: grid;
        grid: auto / 50ch 1fr;
      }
      </style>
      <label><input id="table" type="checkbox"> Display as table</label>
      <main>
        ${eventsInRoom.map(
          (event) =>
            html`
              <dt><a href="${url`https://matrix.to/#/${event.room_id}/${event.event_id}`}">${event
                .user_id ??
                ""} ${new Date(
                event.origin_server_ts,
              ).toJSON()}</a></dt>
              <dd>${event.content.formatted_body
                ? new HTMLString(event.content.formatted_body)
                : event.content.body ?? ""}</dd>
            `,
        )}
      </main>
    `,
  );
}
