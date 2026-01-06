// COBS encoding/decoding (minimal, no external deps).
export function cobsEncode(input: Uint8Array): Uint8Array {
  const out: number[] = [];
  let codeIndex = 0;
  let code = 1;
  out.push(0); // code placeholder

  for (let i = 0; i < input.length; i++) {
    const b = input[i]!;
    if (b === 0) {
      out[codeIndex] = code;
      codeIndex = out.length;
      out.push(0);
      code = 1;
    } else {
      out.push(b);
      code++;
      if (code === 0xff) {
        out[codeIndex] = code;
        codeIndex = out.length;
        out.push(0);
        code = 1;
      }
    }
  }

  out[codeIndex] = code;
  return Uint8Array.from(out);
}

export function cobsDecode(input: Uint8Array): Uint8Array {
  const out: number[] = [];
  let i = 0;
  while (i < input.length) {
    const code = input[i++]!;
    if (code === 0) throw new Error("cobs: zero code");
    const n = code - 1;
    if (i + n > input.length) throw new Error("cobs: overrun");
    for (let j = 0; j < n; j++) out.push(input[i++]!);
    if (code !== 0xff && i < input.length) out.push(0);
  }
  return Uint8Array.from(out);
}

