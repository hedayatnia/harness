import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  decodeFrame,
  encodeFrame,
  FRAME,
  type FrameType,
  MAX_HEADER_BYTES,
} from "./chat-frames";

/** The server's half of the chat2 conformance vectors
 * (`protocol/chat2-vectors.json`, also run by the Rust and Swift clients).
 * Catch-up planning and the row cursor are client rules; the server checks
 * the codec. */

interface Vectors {
  frameTypes: Record<string, number>;
  maxHeaderBytes: number;
  encode: {
    name: string;
    kind: number;
    header: Record<string, unknown>;
    payloadHex: string;
    bytesHex: string;
  }[];
  decodeRejects: { name: string; bytesHex: string }[];
  decodeAccepts: {
    name: string;
    kind: number;
    header: Record<string, unknown>;
    payloadHex: string;
    bytesHex: string;
    clientOnly?: boolean;
  }[];
}

const vectors: Vectors = JSON.parse(
  readFileSync(
    fileURLToPath(
      new URL("../../protocol/chat2-vectors.json", import.meta.url).href,
    ),
    "utf8",
  ),
);

const hex = (s: string): Uint8Array =>
  Uint8Array.from(s.match(/../g) ?? [], (b) => parseInt(b, 16));

describe("chat2 conformance vectors", () => {
  it("uses the same frame type bytes and header cap", () => {
    expect(FRAME).toEqual(vectors.frameTypes);
    expect(MAX_HEADER_BYTES).toBe(vectors.maxHeaderBytes);
  });

  for (const c of vectors.encode) {
    it(`encodes exact bytes: ${c.name}`, () => {
      const bytes = encodeFrame(
        c.kind as FrameType,
        c.header,
        hex(c.payloadHex),
      );
      expect([...bytes]).toEqual([...hex(c.bytesHex)]);
      const frame = decodeFrame(bytes);
      expect(frame?.type).toBe(c.kind);
      expect(frame?.header).toEqual(c.header);
      expect([...(frame?.payload ?? [])]).toEqual([...hex(c.payloadHex)]);
    });
  }

  for (const c of vectors.decodeRejects) {
    it(`rejects: ${c.name}`, () => {
      expect(decodeFrame(hex(c.bytesHex))).toBeUndefined();
    });
  }

  for (const c of vectors.decodeAccepts) {
    it(`${c.clientOnly ? "rejects (client-only accept)" : "accepts"}: ${c.name}`, () => {
      const frame = decodeFrame(hex(c.bytesHex));
      if (c.clientOnly) {
        expect(frame).toBeUndefined();
        return;
      }
      expect(frame?.type).toBe(c.kind);
      expect(frame?.header).toEqual(c.header);
      expect([...(frame?.payload ?? [])]).toEqual([...hex(c.payloadHex)]);
    });
  }
});
