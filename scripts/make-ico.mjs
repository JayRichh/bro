// Pack PNGs into one .ico (PNG-compressed entries, Vista+).
// usage: node scripts/make-ico.mjs out.ico a-16.png a-32.png ...
import { readFileSync, writeFileSync } from "node:fs";
const [out, ...inputs] = process.argv.slice(2);
const images = inputs.map((p) => {
  const data = readFileSync(p);
  return { data, w: data.readUInt32BE(16), h: data.readUInt32BE(20) };
});
const header = Buffer.alloc(6 + 16 * images.length);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(images.length, 4);
let offset = header.length;
images.forEach((img, i) => {
  const e = 6 + 16 * i;
  header.writeUInt8(img.w >= 256 ? 0 : img.w, e);
  header.writeUInt8(img.h >= 256 ? 0 : img.h, e + 1);
  header.writeUInt16LE(1, e + 4);
  header.writeUInt16LE(32, e + 6);
  header.writeUInt32LE(img.data.length, e + 8);
  header.writeUInt32LE(offset, e + 12);
  offset += img.data.length;
});
writeFileSync(out, Buffer.concat([header, ...images.map((i) => i.data)]));
console.log(`wrote ${out} (${images.length} sizes)`);
