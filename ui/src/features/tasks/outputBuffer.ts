import type { OutputChunk } from "../../runtime/contracts";

export const OUTPUT_CHARACTER_LIMIT = 65536;
const streams = ["stdout", "stderr"] as const;
export type OutputState = {
  stdout: string; stderr: string; loadedBytes: number; afterSequence: number;
  hasGaps: boolean; complete: boolean; omittedCharacters: { stdout: number; stderr: number };
};

/** Only presentation tails live here. Durable bytes and cursors belong to worker. */
export class OutputBuffer {
  private decoder = { stdout: new TextDecoder(), stderr: new TextDecoder() };
  private offsets = { stdout: 0, stderr: 0 };
  private state: OutputState = { stdout: "", stderr: "", loadedBytes: 0, afterSequence: 0, hasGaps: false, complete: false, omittedCharacters: { stdout: 0, stderr: 0 } };

  append(chunks: OutputChunk[]) {
    if (!chunks.length) return;
    // Validate the whole page before changing a decoder/cursor: a failed read can retry safely.
    let sequence = this.state.afterSequence;
    for (const chunk of chunks) {
      if (!Number.isSafeInteger(chunk.sequence) || chunk.sequence <= sequence || !Number.isSafeInteger(chunk.offset) || chunk.offset < 0
        || !streams.includes(chunk.stream) || !Array.isArray(chunk.bytes) || !chunk.bytes.length
        || !chunk.bytes.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) throw new Error("Некорректная страница output: sequence, offset или bytes");
      sequence = chunk.sequence;
    }
    if (this.state.complete) throw new Error("Worker добавил output после подтверждённого конца потока");
    const addition = { stdout: "", stderr: "" };
    for (const chunk of chunks) {
      if (chunk.sequence !== this.state.afterSequence + 1) {
        this.state.hasGaps = true;
        for (const stream of streams) { this.decoder[stream] = new TextDecoder(); addition[stream] += "\n[пропуск вывода]\n"; }
      }
      if (chunk.offset !== this.offsets[chunk.stream]) {
        this.state.hasGaps = true;
        this.decoder[chunk.stream] = new TextDecoder();
        addition[chunk.stream] += "\n[пропуск вывода]\n";
      }
      addition[chunk.stream] += this.decoder[chunk.stream].decode(new Uint8Array(chunk.bytes), { stream: true });
      this.offsets[chunk.stream] = chunk.offset + chunk.bytes.length;
      this.state.loadedBytes += chunk.bytes.length;
      this.state.afterSequence = chunk.sequence;
    }
    this.trim(addition);
  }
  finish() {
    if (this.state.complete) return;
    this.trim({ stdout: this.decoder.stdout.decode(), stderr: this.decoder.stderr.decode() });
    this.state.complete = true;
  }
  private trim(addition: { stdout: string; stderr: string }) {
    for (const stream of streams) {
      const characters = Array.from(this.state[stream] + addition[stream]);
      const omitted = Math.max(0, characters.length - OUTPUT_CHARACTER_LIMIT);
      this.state.omittedCharacters[stream] += omitted;
      this.state[stream] = characters.slice(omitted).join("");
    }
  }
  snapshot(): OutputState { return { ...this.state, omittedCharacters: { ...this.state.omittedCharacters } }; }
}
