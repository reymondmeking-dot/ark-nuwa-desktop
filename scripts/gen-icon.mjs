// Generate icons/icon.ico (and PNG sizes) with zero external deps (built-in zlib).
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";

function makePng(size) {
  const W = size, H = size;
  const px = Buffer.alloc(W * H * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      const i = (y * W + x) * 4;
      const dx = x - W / 2, dy = y - H / 2;
      const inCircle = Math.sqrt(dx * dx + dy * dy) < W * 0.4;
      const t = x / W;
      if (inCircle) {
        px[i] = Math.round(91 + t * 33);
        px[i + 1] = Math.round(140 - t * 48);
        px[i + 2] = 255;
        px[i + 3] = 255;
      } else {
        px[i] = 15; px[i + 1] = 17; px[i + 2] = 21; px[i + 3] = 255;
      }
    }
  }
  const crcTable = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  const crc32 = (b) => {
    let c = 0xffffffff;
    for (let i = 0; i < b.length; i++) c = crcTable[(c ^ b[i]) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length, 0);
    const tb = Buffer.from(type, "ascii");
    const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(Buffer.concat([tb, data])), 0);
    return Buffer.concat([len, tb, data, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4);
  ihdr[8] = 8; ihdr[9] = 6;
  const raw = Buffer.alloc((W * 4 + 1) * H);
  for (let y = 0; y < H; y++) {
    raw[y * (W * 4 + 1)] = 0;
    px.copy(raw, y * (W * 4 + 1) + 1, y * W * 4, (y + 1) * W * 4);
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// Build an ICO embedding PNG images (Vista+ supports PNG-in-ICO).
function makeIco(sizes) {
  const images = sizes.map((s) => ({ size: s, data: makePng(s) }));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type icon
  header.writeUInt16LE(images.length, 4);
  let offset = 6 + images.length * 16;
  const entries = [];
  const datas = [];
  for (const img of images) {
    const e = Buffer.alloc(16);
    e[0] = img.size >= 256 ? 0 : img.size; // width
    e[1] = img.size >= 256 ? 0 : img.size; // height
    e[2] = 0; e[3] = 0;
    e.writeUInt16LE(1, 4);  // planes
    e.writeUInt16LE(32, 6); // bpp
    e.writeUInt32LE(img.data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += img.data.length;
    entries.push(e);
    datas.push(img.data);
  }
  return Buffer.concat([header, ...entries, ...datas]);
}

const dir = process.argv[2] || "src-tauri/icons";
mkdirSync(dir, { recursive: true });
writeFileSync(`${dir}/icon.ico`, makeIco([16, 32, 48, 256]));
writeFileSync(`${dir}/icon.png`, makePng(512));
writeFileSync(`${dir}/32x32.png`, makePng(32));
writeFileSync(`${dir}/128x128.png`, makePng(128));
writeFileSync(`${dir}/128x128@2x.png`, makePng(256));
console.log("icons written to", dir);
