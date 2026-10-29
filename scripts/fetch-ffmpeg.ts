// Télécharge le binaire FFmpeg embarqué dans l'application (sidecar Tauri).
//
//   bun scripts/fetch-ffmpeg.ts [--target <triple>] [--force]
//
// Le fichier est écrit dans src-tauri/binaries/ffmpeg-<triple>[.exe], nom exigé
// par `bundle.externalBin`. Variables : FFMPEG_URL pour forcer une autre source
// (archive .zip/.tar.xz/.gz), SKIP_FFMPEG_FETCH=1 pour ne rien faire.
//
// Licences (voir docs/ffmpeg.md) : Windows et Linux utilisent les builds LGPL de
// BtbN ; macOS utilise les builds statiques d'eugeneware/ffmpeg-static (GPL).
// Omnistream n'utilise que `-c copy` : aucun codec GPL n'est nécessaire.

import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const outDir = join(root, "src-tauri", "binaries");
const args = process.argv.slice(2);
const force = args.includes("--force");

if (process.env.SKIP_FFMPEG_FETCH === "1") process.exit(0);

async function hostTriple(): Promise<string> {
  const out = await Bun.$`rustc -vV`.text();
  const m = out.match(/^host: (.+)$/m);
  if (!m) throw new Error("Impossible de déterminer la cible Rust (rustc -vV).");
  return m[1].trim();
}

const flag = args.indexOf("--target");
const triple = (flag >= 0 && args[flag + 1]) || process.env.TAURI_ENV_TARGET_TRIPLE || (await hostTriple());

const BTBN = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";
const STATIC = "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.0";

type Kind = "zip" | "tar.xz" | "gz";
interface Source {
  url: string;
  kind: Kind;
}

const SOURCES: Record<string, Source> = {
  "x86_64-pc-windows-msvc": { url: `${BTBN}/ffmpeg-master-latest-win64-lgpl.zip`, kind: "zip" },
  "aarch64-pc-windows-msvc": { url: `${BTBN}/ffmpeg-master-latest-winarm64-lgpl.zip`, kind: "zip" },
  "x86_64-unknown-linux-gnu": { url: `${BTBN}/ffmpeg-master-latest-linux64-lgpl.tar.xz`, kind: "tar.xz" },
  "aarch64-unknown-linux-gnu": { url: `${BTBN}/ffmpeg-master-latest-linuxarm64-lgpl.tar.xz`, kind: "tar.xz" },
  "x86_64-apple-darwin": { url: `${STATIC}/ffmpeg-darwin-x64.gz`, kind: "gz" },
  "aarch64-apple-darwin": { url: `${STATIC}/ffmpeg-darwin-arm64.gz`, kind: "gz" },
};

const exe = triple.includes("windows") ? ".exe" : "";
const dest = join(outDir, `ffmpeg-${triple}${exe}`);

if (existsSync(dest) && !force) {
  console.log(`FFmpeg déjà présent : ${dest}`);
  process.exit(0);
}

const override = process.env.FFMPEG_URL;
const kindOf = (url: string): Kind => (url.endsWith(".zip") ? "zip" : url.endsWith(".gz") && !url.endsWith(".tar.gz") ? "gz" : "tar.xz");
const source: Source | undefined = override ? { url: override, kind: kindOf(override) } : SOURCES[triple];
if (!source) {
  console.error(`Aucune source FFmpeg connue pour ${triple}. Définissez FFMPEG_URL ou placez le binaire dans ${dest}.`);
  process.exit(1);
}

mkdirSync(outDir, { recursive: true });
const work = mkdtempSync(join(tmpdir(), "omnistream-ffmpeg-"));
try {
  console.log(`Téléchargement de ${source.url}…`);
  const res = await fetch(source.url, { redirect: "follow" });
  if (!res.ok) throw new Error(`Téléchargement impossible (${res.status}) : ${source.url}`);
  const archive = join(work, "archive");
  await Bun.write(archive, res);

  let binary: string;
  if (source.kind === "gz") {
    binary = join(work, "ffmpeg");
    await Bun.write(binary, Bun.gunzipSync(new Uint8Array(await Bun.file(archive).arrayBuffer())));
  } else {
    // bsdtar (Windows 10+, macOS) et GNU tar lisent tous deux ces formats.
    const extractDir = join(work, "extracted");
    mkdirSync(extractDir);
    await Bun.$`tar -xf ${archive} -C ${extractDir}`;
    const found = [...new Bun.Glob(`**/bin/ffmpeg${exe}`).scanSync({ cwd: extractDir, absolute: true })][0];
    if (!found) throw new Error("ffmpeg introuvable dans l'archive téléchargée.");
    binary = found;
  }

  copyFileSync(binary, dest);
  if (!exe) chmodSync(dest, 0o755);
  console.log(`FFmpeg installé : ${dest}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
