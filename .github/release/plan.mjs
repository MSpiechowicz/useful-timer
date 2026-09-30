import { appendFile } from "node:fs/promises";
import semanticRelease from "semantic-release";
import config from "../../.releaserc.json" with { type: "json" };

// Planning does not need GitHub publishing or release-commit preparation.
const result = await semanticRelease({
  dryRun: true,
  plugins: config.plugins.slice(0, 2),
});

await appendFile(
  process.env.GITHUB_OUTPUT,
  `version=${result ? result.nextRelease.version : ""}\n`,
);
