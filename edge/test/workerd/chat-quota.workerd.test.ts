import { env } from "cloudflare:test";
import { expect, it } from "vitest";
import { decodeFrame, encodeFrame, FRAME, type Frame } from "../../src/chat-frames";
import { AUTH_USER_HEADER } from "../../src/env";

/** #344: the push quota belongs to one (chat, device) pair, and a refusal
 * says when that pair's window reopens. */

const QUOTA_MAX_PUSHES = 300;
const QUOTA_WINDOW_MS = 60_000;

const room = (chat: string) => env.CHAT_ROOMS.get(env.CHAT_ROOMS.idFromName(`chat2/${chat}`));

const push = (chat: string, device: string, batchId: string): Promise<Response> =>
  room(chat).fetch(`https://chat/rows?device=${device}&batchId=${batchId}`, {
    method: "POST",
    headers: { [AUTH_USER_HEADER]: "user" },
    body: new Uint8Array([1, 2, 3])
  });

it("refuses an exhausted chat's HTTP push with a retry hint, and only that chat and device", async () => {
  for (let i = 0; i < QUOTA_MAX_PUSHES; i++) {
    const ok = await push("quota-http", "mac", `b${i}`);
    expect(ok.status).toBe(200);
    await ok.arrayBuffer();
  }
  const refused = await push("quota-http", "mac", "over");
  expect(refused.status).toBe(429);
  const body = (await refused.json()) as {
    error: string;
    retryAfterMs: number;
  };
  expect(body.error).toBe("quota");
  expect(body.retryAfterMs).toBeGreaterThan(0);
  expect(body.retryAfterMs).toBeLessThanOrEqual(QUOTA_WINDOW_MS);
  expect(refused.headers.get("retry-after")).toBe(String(Math.ceil(body.retryAfterMs / 1000)));

  // The same device in another chat, and another device in this chat, each
  // have their own window.
  for (const [chat, device] of [
    ["quota-http-other", "mac"],
    ["quota-http", "phone"]
  ]) {
    const admitted = await push(chat!, device!, "fresh");
    expect(admitted.status).toBe(200);
    await admitted.arrayBuffer();
  }
});

it("puts the retry hint on the WebSocket quota error", async () => {
  const response = await room("quota-ws").fetch("https://chat/ws?device=mac", {
    headers: { upgrade: "websocket", [AUTH_USER_HEADER]: "user" }
  });
  const ws = response.webSocket!;
  ws.accept();
  // The test's side of the socket receives binary messages as Blobs.
  const received: Blob[] = [];
  ws.addEventListener("message", (event) => received.push(event.data as unknown as Blob));
  ws.send(encodeFrame(FRAME.hello, { cursor: 0, device: "mac" }));
  for (let i = 0; i <= QUOTA_MAX_PUSHES; i++) {
    ws.send(encodeFrame(FRAME.push, { batchId: `w${i}` }, new Uint8Array([i & 0xff])));
  }
  for (let spins = 0; received.length < QUOTA_MAX_PUSHES + 2 && spins < 500; spins++) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  const frames: Frame[] = await Promise.all(
    received.map(async (blob) => decodeFrame(new Uint8Array(await blob.arrayBuffer()))!)
  );
  expect(frames.filter((f) => f.type === FRAME.ack)).toHaveLength(QUOTA_MAX_PUSHES);
  const refusal = frames.at(-1)!;
  expect(refusal.type).toBe(FRAME.error);
  expect(refusal.header).toMatchObject({
    code: "quota",
    batchId: `w${QUOTA_MAX_PUSHES}`
  });
  expect(refusal.header.retryAfterMs).toBeGreaterThan(0);
  expect(refusal.header.retryAfterMs).toBeLessThanOrEqual(QUOTA_WINDOW_MS);
  ws.close();
});
