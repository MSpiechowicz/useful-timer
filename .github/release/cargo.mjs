import { readFile, stat, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { join } from "node:path";

export async function verifyRelease(_config, { cwd, nextRelease, env }) {
  if (nextRelease.version !== env.RELEASE_VERSION) {
    throw new Error(
      `Release version ${nextRelease.version} does not match built artifacts (${env.RELEASE_VERSION}).`,
    );
  }

  for (const target of [
    "x86_64-unknown-linux-gnu.tar.gz",
    "x86_64-apple-darwin.tar.gz",
    "aarch64-apple-darwin.tar.gz",
    "x86_64-pc-windows-msvc.zip",
  ]) {
    const asset = join(cwd, "dist", `useful-timer-${nextRelease.version}-${target}`);
    const info = await stat(asset);
    if (!info.isFile() || info.size === 0) {
      throw new Error(`Missing or empty release archive: ${asset}`);
    }
    const checksum = await readFile(`${asset}.sha256`, "utf8");
    const hash = createHash("sha256").update(await readFile(asset)).digest("hex");
    const filename = `useful-timer-${nextRelease.version}-${target}`;
    if (checksum.trim() !== `${hash}  ${filename}`) {
      throw new Error(`Invalid release checksum: ${asset}.sha256`);
    }
  }
}

export async function prepare(_config, { cwd = process.cwd(), nextRelease }) {
  const version = nextRelease.version;
  const updates = [
    ["Cargo.toml", /(\[package\][\s\S]*?\r?\nversion = ")[^"]+/],
    ["Cargo.lock", /(\[\[package\]\]\r?\nname = "useful-timer"\r?\nversion = ")[^"]+/],
  ];

  const files = await Promise.all(
    updates.map(async ([name, pattern]) => {
      const path = join(cwd, name);
      const source = await readFile(path, "utf8");
      if (!pattern.test(source)) {
        throw new Error(`Cannot find useful-timer package version in ${name}.`);
      }
      return [path, source.replace(pattern, (_match, prefix) => `${prefix}${version}`)];
    }),
  );

  await Promise.all(files.map(([path, content]) => writeFile(path, content)));
}
