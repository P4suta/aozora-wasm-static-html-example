const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: false });

export function decodeUtf8(bytes: Uint8Array, label: string): string {
  let value: string;
  try {
    value = decoder.decode(bytes);
  } catch (error) {
    throw new Error(`${label} is not valid UTF-8`, { cause: error });
  }

  if (value.includes("\uFFFD")) {
    throw new Error(`${label} contains a Unicode replacement character`);
  }
  return value;
}
