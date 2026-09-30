import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { prepare, verifyRelease } from "./cargo.mjs";

for (const newline of ["\n", "\r\n"]) {
  test(`stamp Cargo versions without changing dependencies or ${JSON.stringify(newline)} line endings`, async (t) => {
    const cwd = await mkdtemp(join(tmpdir(), "useful-timer-release-"));
    t.after(() => rm(cwd, { recursive: true, force: true }));
    const manifest = '[package]\nname = "useful-timer"\nversion = "0.1.0"\n\n[dependencies]\nexample = "1.2.3"\n'.replaceAll("\n", newline);
    const lockfile = 'version = 4\n\n[[package]]\nname = "example"\nversion = "1.2.3"\n\n[[package]]\nname = "useful-timer"\nversion = "0.1.0"\ndependencies = ["example"]\n'.replaceAll("\n", newline);
    await writeFile(join(cwd, "Cargo.toml"), manifest);
    await writeFile(join(cwd, "Cargo.lock"), lockfile);
    await prepare({}, { cwd, nextRelease: { version: "2.3.4" } });
    assert.equal(await readFile(join(cwd, "Cargo.toml"), "utf8"), manifest.replace('version = "0.1.0"', 'version = "2.3.4"'));
    assert.equal(await readFile(join(cwd, "Cargo.lock"), "utf8"), lockfile.replace('version = "0.1.0"', 'version = "2.3.4"'));
  });
}

test("invalid lockfile does not partially stamp the manifest", async (t) => {
  const cwd = await mkdtemp(join(tmpdir(), "useful-timer-release-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  const manifest = '[package]\nversion = "0.1.0"\n';
  await writeFile(join(cwd, "Cargo.toml"), manifest);
  await writeFile(join(cwd, "Cargo.lock"), '[[package]]\nname = "another-app"\nversion = "0.1.0"\n');
  await assert.rejects(prepare({}, { cwd, nextRelease: { version: "2.3.4" } }), /Cargo.lock/);
  assert.equal(await readFile(join(cwd, "Cargo.toml"), "utf8"), manifest);
});

test("publish requires all four archives with matching checksums", async (t) => {
  const cwd = await mkdtemp(join(tmpdir(), "useful-timer-release-"));
  t.after(() => rm(cwd, { recursive: true, force: true }));
  await mkdir(join(cwd, "dist"));
  const context = { cwd, nextRelease: { version: "2.3.4" }, env: { RELEASE_VERSION: "2.3.4" } };
  const targets = ["x86_64-unknown-linux-gnu.tar.gz", "x86_64-apple-darwin.tar.gz", "aarch64-apple-darwin.tar.gz", "x86_64-pc-windows-msvc.zip"];
  for (const target of targets) {
    const filename = `useful-timer-2.3.4-${target}`;
    const content = Buffer.from(`archive for ${target}`);
    await writeFile(join(cwd, "dist", filename), content);
    const hash = createHash("sha256").update(content).digest("hex");
    await writeFile(join(cwd, "dist", `${filename}.sha256`), `${hash}  ${filename}\r\n`);
  }
  await verifyRelease({}, context);
  const apple = join(cwd, "dist", "useful-timer-2.3.4-aarch64-apple-darwin.tar.gz");
  await writeFile(apple, "corrupted download");
  await assert.rejects(verifyRelease({}, context), /Invalid release checksum/);
  await rm(apple);
  await assert.rejects(verifyRelease({}, context), /ENOENT/);
  await assert.rejects(verifyRelease({}, { ...context, env: { RELEASE_VERSION: "2.3.5" } }), /does not match/);
});
