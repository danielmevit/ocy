import fs from 'node:fs';
import path from 'node:path';

export interface PngDimensions {
  width: number;
  height: number;
}

const PNG_SIG = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/**
 * Reads width and height from a PNG file's IHDR chunk without loading the image.
 * Returns null if the file is absent or not a valid PNG.
 */
export function readPngSize(publicRelativePath: string): PngDimensions | null {
  const cleanRel = publicRelativePath.replace(/^\/+/, '');
  const cwd = process.cwd();
  const publicDir = fs.existsSync(path.join(cwd, 'public'))
    ? path.join(cwd, 'public')
    : path.join(cwd, 'site', 'public');
  const fullPath = path.join(publicDir, cleanRel);
  if (!fs.existsSync(fullPath)) {
    return null;
  }

  let fd: number | undefined;
  try {
    fd = fs.openSync(fullPath, 'r');
    const buf = Buffer.alloc(24);
    const bytesRead = fs.readSync(fd, buf, 0, 24, 0);
    if (bytesRead < 24) return null;

    for (let i = 0; i < PNG_SIG.length; i++) {
      if (buf[i] !== PNG_SIG[i]) return null;
    }
    if (buf.toString('ascii', 12, 16) !== 'IHDR') return null;

    const width = buf.readUInt32BE(16);
    const height = buf.readUInt32BE(20);
    if (width === 0 || height === 0) return null;
    return { width, height };
  } catch {
    return null;
  } finally {
    if (fd !== undefined) {
      fs.closeSync(fd);
    }
  }
}
